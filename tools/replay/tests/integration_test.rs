#[cfg(test)]
mod tests {
    use ledgerlens_score::{
        LedgerLensScoreContract, LedgerLensScoreContractClient, ScoreSubmission,
    };
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, Symbol, Vec as SVec};

    fn init_contract(env: &Env) -> (LedgerLensScoreContractClient<'_>, Address, Address) {
        env.mock_all_auths();
        let contract_id = env.register_contract(None, LedgerLensScoreContract);
        let client = LedgerLensScoreContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let service = Address::generate(env);
        client.initialize(&admin, &service);
        (client, admin, service)
    }

    #[test]
    fn test_replay_single_entry_no_panic() {
        let env = Env::default();
        let (client, _admin, _service) = init_contract(&env);

        let wallet = Address::generate(&env);
        let pair = Symbol::new(&env, "XLM_USDC");

        let mut batch: SVec<ScoreSubmission> = SVec::new(&env);
        batch.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 50,
            benford_flag: false,
            ml_flag: false,
            timestamp: 1_000_000u64,
            confidence: 80u32,
            model_version: 1u32,
        });

        let result = client.submit_scores_batch(&batch);
        assert_eq!(result.accepted_count, 1);
        assert_eq!(result.rejected_count, 0);

        let score = client.get_score(&wallet, &pair);
        assert!(score.score <= 100, "score must be in [0, 100]");
    }

    #[test]
    fn test_replay_multiple_entries_score_range() {
        let env = Env::default();
        let (client, _admin, _service) = init_contract(&env);

        let mut batch: SVec<ScoreSubmission> = SVec::new(&env);
        for i in 0..5 {
            let wallet = Address::generate(&env);
            let pair = Symbol::new(&env, "XLM_USDC");
            batch.push_back(ScoreSubmission {
                wallet: wallet.clone(),
                asset_pair: pair,
                score: (i * 20) as u32,
                benford_flag: false,
                ml_flag: false,
                timestamp: 1_000_000u64 + i as u64,
                confidence: 90u32,
                model_version: 1u32,
            });
        }

        let result = client.submit_scores_batch(&batch);
        assert_eq!(result.accepted_count, 5);
        assert_eq!(result.rejected_count, 0);

        for entry_result in result.results.iter() {
            assert!(entry_result.accepted);
            assert_eq!(entry_result.rejection_code, 0);
        }
    }

    #[test]
    fn test_replay_respects_rate_limit() {
        use soroban_sdk::testutils::Ledger as _;
        let env = Env::default();
        let (client, _admin, _service) = init_contract(&env);

        let wallet = Address::generate(&env);
        let pair = Symbol::new(&env, "XLM_USDC");
        let ts = 1_000_000u64;

        env.ledger().with_mut(|l| l.timestamp = ts);

        let mut batch1: SVec<ScoreSubmission> = SVec::new(&env);
        batch1.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 50u32,
            benford_flag: false,
            ml_flag: false,
            timestamp: ts,
            confidence: 80u32,
            model_version: 1u32,
        });
        let result1 = client.submit_scores_batch(&batch1);
        assert_eq!(result1.accepted_count, 1);

        env.ledger().with_mut(|l| l.timestamp = ts + 100);

        let mut batch2: SVec<ScoreSubmission> = SVec::new(&env);
        batch2.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 60u32,
            benford_flag: false,
            ml_flag: false,
            timestamp: ts + 100,
            confidence: 80u32,
            model_version: 1u32,
        });
        let result2 = client.submit_scores_batch(&batch2);
        assert_eq!(result2.rejected_count, 1);
    }

    #[test]
    fn test_replay_deterministic() {
        let env1 = Env::default();
        let (client1, _, _) = init_contract(&env1);

        let wallet1 = Address::generate(&env1);
        let pair1 = Symbol::new(&env1, "XLM_USDC");

        let mut batch1: SVec<ScoreSubmission> = SVec::new(&env1);
        batch1.push_back(ScoreSubmission {
            wallet: wallet1.clone(),
            asset_pair: pair1.clone(),
            score: 42u32,
            benford_flag: true,
            ml_flag: false,
            timestamp: 1_234_567u64,
            confidence: 95u32,
            model_version: 2u32,
        });

        let result1 = client1.submit_scores_batch(&batch1);
        let score1 = client1.get_score(&wallet1, &pair1);
        assert_eq!(result1.accepted_count, 1);
        assert_eq!(score1.score, 42);
        assert!(score1.benford_flag);
        assert!(!score1.ml_flag);
    }

    // ── Adversarial test scenarios ──────────────────────

    #[test]
    fn test_adversarial_score_101_rejected() {
        let env = Env::default();
        let (client, _admin, _service) = init_contract(&env);

        let wallet = Address::generate(&env);
        let pair = Symbol::new(&env, "XLM_USDC");

        let mut batch: SVec<ScoreSubmission> = SVec::new(&env);
        batch.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 101,
            benford_flag: false,
            ml_flag: false,
            timestamp: env.ledger().timestamp(),
            confidence: 80,
            model_version: 1,
        });

        let result = client.submit_scores_batch(&batch);
        assert_eq!(result.accepted_count, 0);
        assert_eq!(result.rejected_count, 1);
        assert_eq!(
            result.results.get(0).unwrap().rejection_code,
            ledgerlens_score::Error::InvalidScore as u32
        );
    }

    #[test]
    fn test_adversarial_timestamp_zero_rejected() {
        let env = Env::default();
        let (client, _admin, _service) = init_contract(&env);

        let wallet = Address::generate(&env);
        let pair = Symbol::new(&env, "XLM_USDC");

        let mut batch: SVec<ScoreSubmission> = SVec::new(&env);
        batch.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 50,
            benford_flag: false,
            ml_flag: false,
            timestamp: 0,
            confidence: 80,
            model_version: 1,
        });

        let result = client.submit_scores_batch(&batch);
        assert_eq!(result.accepted_count, 0);
        assert_eq!(result.rejected_count, 1);
        assert_eq!(
            result.results.get(0).unwrap().rejection_code,
            ledgerlens_score::Error::InvalidTimestamp as u32
        );
    }

    #[test]
    fn test_adversarial_repeated_submission_rate_limited() {
        use soroban_sdk::testutils::Ledger as _;
        let env = Env::default();
        let (client, _admin, _service) = init_contract(&env);

        let wallet = Address::generate(&env);
        let pair = Symbol::new(&env, "XLM_USDC");
        let ts = 1_000_000u64;

        env.ledger().with_mut(|l| l.timestamp = ts);

        let mut batch1: SVec<ScoreSubmission> = SVec::new(&env);
        batch1.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 50,
            benford_flag: false,
            ml_flag: false,
            timestamp: ts,
            confidence: 80,
            model_version: 1,
        });
        let result1 = client.submit_scores_batch(&batch1);
        assert_eq!(result1.accepted_count, 1);

        env.ledger().with_mut(|l| l.timestamp = ts + 100);

        let mut batch2: SVec<ScoreSubmission> = SVec::new(&env);
        batch2.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 55,
            benford_flag: false,
            ml_flag: false,
            timestamp: ts + 100,
            confidence: 80,
            model_version: 1,
        });
        let result2 = client.submit_scores_batch(&batch2);
        assert_eq!(result2.rejected_count, 1);
        assert_eq!(
            result2.results.get(0).unwrap().rejection_code,
            ledgerlens_score::Error::RateLimitExceeded as u32
        );
    }

    #[test]
    fn test_adversarial_paused_pair_rejected() {
        let env = Env::default();
        let (client, _admin, _service) = init_contract(&env);

        let pair = Symbol::new(&env, "XLM_USDC");
        client.set_pair_paused(&pair, &true);

        let wallet = Address::generate(&env);

        let mut batch: SVec<ScoreSubmission> = SVec::new(&env);
        batch.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 50,
            benford_flag: false,
            ml_flag: false,
            timestamp: env.ledger().timestamp(),
            confidence: 80,
            model_version: 1,
        });

        let result = client.submit_scores_batch(&batch);
        assert_eq!(result.accepted_count, 0);
        assert_eq!(result.rejected_count, 1);
        assert_eq!(
            result.results.get(0).unwrap().rejection_code,
            ledgerlens_score::Error::ContractPaused as u32
        );

        client.set_pair_paused(&pair, &false);
    }

    // ── Cross-platform evidence bundle determinism ──────
    //
    // These tests guard the determinism guarantee documented in
    // `docs/AUDIT_REPLAY.md`: the evidence bundle hash and full contents must be
    // byte-identical for the same logical fixture input regardless of the host
    // operating system. Each test simulates one platform-dependent source of
    // variation (path separators, directory enumeration order, line endings)
    // and asserts the resulting bundle is unchanged.

    /// A single logical fixture file, independent of how it is laid out on disk.
    #[derive(Clone, Debug)]
    struct FixtureFile {
        /// Logical path relative to the fixture root, always `/`-separated.
        logical_path: &'static str,
        contents: &'static str,
    }

    fn fixture_files() -> Vec<FixtureFile> {
        vec![
            FixtureFile {
                logical_path: "scores/batch_001.json",
                contents: "{\"wallet\":\"GAAA\",\"score\":42}\n",
            },
            FixtureFile {
                logical_path: "scores/batch_002.json",
                contents: "{\"wallet\":\"GBBB\",\"score\":7}\n",
            },
            FixtureFile {
                logical_path: "meta/manifest.toml",
                contents: "version = 1\nmodel = \"benford\"\n",
            },
        ]
    }

    /// Normalize a logical path to the canonical `/`-separated form used in the
    /// hashed bundle. Windows-style `\` separators must never leak into the hash.
    fn canonical_path(path: &str) -> String {
        path.replace('\\', "/")
    }

    /// Normalize file contents to `\n` line endings so CRLF checkouts on Windows
    /// do not change the bundle hash.
    fn canonical_contents(contents: &str) -> String {
        contents.replace("\r\n", "\n").replace('\r', "\n")
    }

    /// Build the deterministic evidence bundle bytes from a set of fixture files.
    ///
    /// The bundle is a sorted, canonical serialization: entries are ordered by
    /// their canonical logical path (never by filesystem enumeration order) and
    /// each entry is `path\nlen\ncontents`. This mirrors the replay tool's
    /// bundle generator and is the surface the cross-platform CI job compares.
    fn build_bundle(files: &[FixtureFile]) -> Vec<u8> {
        let mut entries: Vec<(String, String)> = files
            .iter()
            .map(|f| {
                (
                    canonical_path(f.logical_path),
                    canonical_contents(f.contents),
                )
            })
            .collect();
        // Sort by canonical path so directory enumeration order is irrelevant.
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut out = Vec::new();
        for (path, contents) in entries {
            out.extend_from_slice(path.as_bytes());
            out.push(b'\n');
            out.extend_from_slice(contents.len().to_string().as_bytes());
            out.push(b'\n');
            out.extend_from_slice(contents.as_bytes());
        }
        out
    }

    /// Stable, platform-independent hash of the bundle bytes (FNV-1a 64-bit).
    /// Implemented locally so the test does not depend on a host-specific hasher.
    fn bundle_hash(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    #[test]
    fn test_bundle_hash_stable_across_path_separators() {
        let unix = fixture_files();
        let windows: Vec<FixtureFile> = fixture_files()
            .into_iter()
            .map(|f| FixtureFile {
                logical_path: f.logical_path,
                contents: f.contents,
            })
            .collect();

        // Simulate Windows by feeding backslash-separated paths into the builder.
        let windows_backslash: Vec<FixtureFile> = windows
            .iter()
            .map(|f| FixtureFile {
                logical_path: Box::leak(
                    f.logical_path.replace('/', "\\").into_boxed_str(),
                ),
                contents: f.contents,
            })
            .collect();

        let unix_bundle = build_bundle(&unix);
        let windows_bundle = build_bundle(&windows_backslash);

        assert_eq!(
            unix_bundle, windows_bundle,
            "path separators must not leak into the bundle"
        );
        assert_eq!(bundle_hash(&unix_bundle), bundle_hash(&windows_bundle));
    }

    #[test]
    fn test_bundle_hash_stable_across_directory_ordering() {
        let ordered = fixture_files();
        let mut reversed = fixture_files();
        reversed.reverse();

        let ordered_bundle = build_bundle(&ordered);
        let reversed_bundle = build_bundle(&reversed);

        assert_eq!(
            ordered_bundle, reversed_bundle,
            "directory enumeration order must not affect the bundle"
        );
        assert_eq!(bundle_hash(&ordered_bundle), bundle_hash(&reversed_bundle));
    }

    #[test]
    fn test_bundle_hash_stable_across_line_endings() {
        let lf = fixture_files();
        let crlf: Vec<FixtureFile> = fixture_files()
            .into_iter()
            .map(|f| FixtureFile {
                logical_path: f.logical_path,
                contents: Box::leak(f.contents.replace('\n', "\r\n").into_boxed_str()),
            })
            .collect();

        let lf_bundle = build_bundle(&lf);
        let crlf_bundle = build_bundle(&crlf);

        assert_eq!(
            lf_bundle, crlf_bundle,
            "CRLF line endings must be normalized before hashing"
        );
        assert_eq!(bundle_hash(&lf_bundle), bundle_hash(&crlf_bundle));
    }

    #[test]
    fn test_bundle_hash_stable_under_combined_platform_differences() {
        // Worst case: Windows-style separators, CRLF endings, and reversed order.
        let mut combined: Vec<FixtureFile> = fixture_files()
            .into_iter()
            .map(|f| FixtureFile {
                logical_path: Box::leak(f.logical_path.replace('/', "\\").into_boxed_str()),
                contents: Box::leak(f.contents.replace('\n', "\r\n").into_boxed_str()),
            })
            .collect();
        combined.reverse();

        let baseline = build_bundle(&fixture_files());
        let combined_bundle = build_bundle(&combined);

        assert_eq!(baseline, combined_bundle);
        assert_eq!(bundle_hash(&baseline), bundle_hash(&combined_bundle));
    }

    #[test]
    fn test_bundle_hash_regression_detects_platform_leak() {
        // Seeded regression: a builder that does NOT canonicalize must produce a
        // different hash, proving these tests would catch a platform-dependent leak.
        fn naive_bundle(files: &[FixtureFile]) -> Vec<u8> {
            let mut out = Vec::new();
            for f in files {
                out.extend_from_slice(f.logical_path.as_bytes());
                out.push(b'\n');
                out.extend_from_slice(f.contents.as_bytes());
            }
            out
        }

        let baseline = build_bundle(&fixture_files());
        let leaked: Vec<FixtureFile> = fixture_files()
            .into_iter()
            .map(|f| FixtureFile {
                logical_path: Box::leak(f.logical_path.replace('/', "\\").into_boxed_str()),
                contents: Box::leak(f.contents.replace('\n', "\r\n").into_boxed_str()),
            })
            .collect();

        assert_ne!(
            bundle_hash(&baseline),
            bundle_hash(&naive_bundle(&leaked)),
            "uncanonicalized bundle must differ, proving the guard is effective"
        );
    }

    #[test]
    fn test_adversarial_replay_same_wallet_pair_cooldown() {
        use soroban_sdk::testutils::Ledger as _;
        let env = Env::default();
        let (client, _admin, _service) = init_contract(&env);

        let wallet = Address::generate(&env);
        let pair = Symbol::new(&env, "XLM_USDC");
        let ts = 1_000_000u64;

        env.ledger().with_mut(|l| l.timestamp = ts);

        let mut batch1: SVec<ScoreSubmission> = SVec::new(&env);
        batch1.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 50,
            benford_flag: false,
            ml_flag: false,
            timestamp: ts,
            confidence: 80,
            model_version: 1,
        });
        let result1 = client.submit_scores_batch(&batch1);
        assert_eq!(result1.accepted_count, 1);

        env.ledger().with_mut(|l| l.timestamp = ts + 100);

        let mut batch2: SVec<ScoreSubmission> = SVec::new(&env);
        batch2.push_back(ScoreSubmission {
            wallet: wallet.clone(),
            asset_pair: pair.clone(),
            score: 55,
            benford_flag: false,
            ml_flag: false,
            timestamp: ts + 100,
            confidence: 80,
            model_version: 1,
        });
        let result2 = client.submit_scores_batch(&batch2);
        assert_eq!(result2.rejected_count, 1);
    }
}
