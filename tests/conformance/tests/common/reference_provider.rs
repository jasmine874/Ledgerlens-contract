#![allow(dead_code)]

//! Adapter binding the conformance harness to the reference provider in
//! `contracts/reference-provider`.
//!
//! This adapter is the "other side" of the suite: it shows what a minimal,
//! independently written provider has to do to claim conformance, and it is the
//! template a third-party provider copies.

use ::reference_provider::{Error, ReferenceRiskRegistry, ReferenceRiskRegistryClient};
use risk_registry_conformance::{ProbeError, ProbeResult, RiskProvider, ScoreView, BASE_TIMESTAMP};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, Symbol,
};

use super::{map, ProviderError};

/// Model version this adapter publishes.
const MODEL_VERSION: u32 = 1;

/// The reference provider has eight error variants and the harness classifies
/// exactly one specially: `ScoreNotFound` is the "no data" answer, not a
/// rejection and emphatically not an unavailability. Getting this backwards is
/// the single easiest way to make a dead provider look safe, which is why the
/// harness asserts the distinction instead of trusting the adapter.
impl ProviderError for Error {
    fn probe_error(&self) -> ProbeError {
        match self {
            Error::ScoreNotFound => ProbeError::NotFound,
            _ => ProbeError::Rejected,
        }
    }
}

/// Drives one `ReferenceRiskRegistry` deployment through the vectors.
pub struct Adapter {
    env: Env,
    /// Address the adapter currently believes the provider lives at.
    registry: Address,
    healthy: Address,
    unreachable: Address,
    scope: Symbol,
}

impl Adapter {
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let healthy = deploy(&env);
        let unreachable = Address::generate(&env);
        Adapter {
            env,
            registry: healthy.clone(),
            healthy,
            unreachable,
            scope: symbol_short!("XLM_USDC"),
        }
    }
}

impl Default for Adapter {
    fn default() -> Self {
        Adapter::new()
    }
}

impl RiskProvider for Adapter {
    fn provider_id(&self) -> &str {
        "reference-provider"
    }

    fn reset(&mut self) -> Address {
        self.env = Env::default();
        self.env.mock_all_auths();
        self.healthy = deploy(&self.env);
        self.registry = self.healthy.clone();
        Address::generate(&self.env)
    }

    fn seed_score(&mut self, score: u32, confidence: u32, age_secs: u64) -> Address {
        let subject = Address::generate(&self.env);
        self.env.ledger().with_mut(|ledger| ledger.timestamp = BASE_TIMESTAMP);
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        client.set_score(
            &subject,
            &self.scope,
            &score,
            &confidence,
            &BASE_TIMESTAMP,
            &MODEL_VERSION,
        );
        if age_secs > 0 {
            self.env.ledger().with_mut(|ledger| ledger.timestamp = BASE_TIMESTAMP + age_secs);
        }
        subject
    }

    fn now(&self) -> u64 {
        self.env.ledger().timestamp()
    }

    fn make_unavailable(&mut self) {
        self.registry = self.unreachable.clone();
    }

    fn make_available(&mut self) {
        self.registry = self.healthy.clone();
    }

    fn gate(&mut self, subject: &Address, threshold: u32) -> ProbeResult<bool> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        map(client.try_query_risk_gate(subject, &self.scope, &threshold))
    }

    fn gate_with_confidence(
        &mut self,
        subject: &Address,
        threshold: u32,
        min_confidence: u32,
    ) -> ProbeResult<bool> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        map(client.try_query_risk_gate_with_confidence(
            subject,
            &self.scope,
            &threshold,
            &min_confidence,
        ))
    }

    fn score(&mut self, subject: &Address) -> ProbeResult<ScoreView> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        let score = map(client.try_get_score(subject, &self.scope))?;
        Ok(ScoreView {
            score: score.score,
            confidence: score.confidence,
            timestamp: score.timestamp,
            model_version: score.model_version,
        })
    }

    fn supports(&mut self, capability: &str) -> ProbeResult<bool> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        let symbol = Symbol::new(&self.env, capability);
        map(client.try_supports_interface(&symbol))
    }

    fn metadata_capabilities(&mut self) -> ProbeResult<std::vec::Vec<String>> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        let metadata = map(client.try_get_interface_metadata())?;
        let capabilities = metadata.capabilities.iter().map(|symbol| symbol.to_string()).collect();
        Ok(capabilities)
    }

    fn interface_version(&mut self) -> ProbeResult<u32> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        let metadata = map(client.try_get_interface_metadata())?;
        Ok(metadata.interface_version)
    }

    fn event_count(&mut self) -> usize {
        self.env.events().all().len() as usize
    }
}

/// Extra probes used by the requirement-coverage tests.
///
/// These are deliberately kept off the [`RiskProvider`] trait: the trait is the
/// contract the conformance harness speaks, while these helpers expose the
/// reference provider's own surface (raw error variants, direct reads, event
/// inspection) so the requirement tests can assert on the exact behavior the
/// standard mandates rather than on the harness's normalized view of it.
impl Adapter {
    /// The scope symbol the adapter seeds and queries with.
    pub fn scope(&self) -> Symbol {
        self.scope.clone()
    }

    /// The address the adapter currently believes the provider lives at.
    pub fn registry(&self) -> Address {
        self.registry.clone()
    }

    /// The address of the healthy deployment (used to prove `make_unavailable`
    /// really points somewhere else).
    pub fn healthy(&self) -> Address {
        self.healthy.clone()
    }

    /// The address used to simulate an unreachable provider.
    pub fn unreachable(&self) -> Address {
        self.unreachable.clone()
    }

    /// The environment the adapter drives.
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// Raw error from `set_score`, preserving the concrete variant so tests can
    /// assert on the exact error the standard requires.
    pub fn try_set_score(
        &self,
        subject: &Address,
        score: u32,
        confidence: u32,
        timestamp: u64,
        model_version: u32,
    ) -> Result<(), Error> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        client.try_set_score(
            subject,
            &self.scope,
            &score,
            &confidence,
            &timestamp,
            &model_version,
        )
    }

    /// Raw error from `get_score`, preserving the concrete variant.
    pub fn try_get_score(&self, subject: &Address) -> Result<::reference_provider::ScoreRecord, Error> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        client.try_get_score(subject, &self.scope)
    }

    /// Raw error from `query_risk_gate`, preserving the concrete variant.
    pub fn try_gate(&self, subject: &Address, threshold: u32) -> Result<bool, Error> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        client.try_query_risk_gate(subject, &self.scope, &threshold)
    }

    /// Raw error from `query_risk_gate_with_confidence`, preserving the
    /// concrete variant.
    pub fn try_gate_with_confidence(
        &self,
        subject: &Address,
        threshold: u32,
        min_confidence: u32,
    ) -> Result<bool, Error> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        client.try_query_risk_gate_with_confidence(
            subject,
            &self.scope,
            &threshold,
            &min_confidence,
        )
    }

    /// Raw error from `supports_interface`, preserving the concrete variant.
    pub fn try_supports(&self, capability: &str) -> Result<bool, Error> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        let symbol = Symbol::new(&self.env, capability);
        client.try_supports_interface(&symbol)
    }

    /// Raw error from `get_interface_metadata`, preserving the concrete variant.
    pub fn try_metadata(&self) -> Result<::reference_provider::InterfaceMetadata, Error> {
        let client = ReferenceRiskRegistryClient::new(&self.env, &self.registry);
        client.try_get_interface_metadata()
    }

    /// Advance the ledger clock without touching any stored score.
    pub fn advance_time(&self, secs: u64) {
        self.env.ledger().with_mut(|ledger| ledger.timestamp += secs);
    }

    /// Set the ledger clock to an absolute timestamp.
    pub fn set_time(&self, timestamp: u64) {
        self.env.ledger().with_mut(|ledger| ledger.timestamp = timestamp);
    }

    /// Number of events emitted so far, as seen by the adapter.
    pub fn events_len(&self) -> usize {
        self.env.events().all().len() as usize
    }

    /// Classify a raw error the same way the harness does, so requirement tests
    /// can assert the "no data" vs "rejected" distinction directly.
    pub fn classify(&self, error: &Error) -> ProbeError {
        error.probe_error()
    }
}

fn deploy(env: &Env) -> Address {
    let contract_id = env.register_contract(None, ReferenceRiskRegistry);
    let client = ReferenceRiskRegistryClient::new(env, &contract_id);
    client.initialize(&Address::generate(env));
    contract_id
}
