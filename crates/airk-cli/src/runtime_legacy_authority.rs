// SPDX-License-Identifier: Apache-2.0
//! Composition of actual held original configuration and actual pinned adopted
//! publication. No serialized assertion can construct this native adapter.
use airk_core::{
    catalog_legacy_adoption::{LegacyAdoptionPlan, LegacyAdoptionReceipt},
    catalog_legacy_authority::StandaloneConfigurationClaim,
    grant_provenance::PersistedGrant,
};
use airk_store::service::registry::{
    ServiceRegistry,
    catalog::{
        adopted_authority::NativeStandaloneAuthority, setup::NativeLegacyAdoptionCompletion,
    },
};
use anyhow::{Result, ensure};

pub(crate) fn configuration_claim(
    origin: &crate::runtime_legacy_origin::NativeLegacyOrigin,
) -> Result<StandaloneConfigurationClaim> {
    let observed = origin.observation();
    let claim = StandaloneConfigurationClaim {
        schema: 1,
        origin_sha256: origin.identity()?,
        config_sha256: observed.config_sha256.parse()?,
        executable_sha256: observed.executable_sha256.parse()?,
        selected_entry_sha256: observed.selected_entry_sha256.parse()?,
        container_path: observed.container_path.clone(),
        original_server: observed.server.clone(),
        no_default_project: observed.declaration.no_default_project,
    };
    claim.validate()?;
    Ok(claim)
}

pub(crate) struct RetainedStandaloneAuthority {
    origins: Vec<crate::runtime_legacy_origin::NativeLegacyOrigin>,
    claims: Vec<StandaloneConfigurationClaim>,
    authority: NativeStandaloneAuthority,
    plan: LegacyAdoptionPlan,
}
impl RetainedStandaloneAuthority {
    pub(crate) fn from_completed_claims(
        mut origins: Vec<crate::runtime_legacy_origin::NativeLegacyOrigin>,
        native: NativeLegacyAdoptionCompletion,
        receipt: LegacyAdoptionReceipt,
    ) -> Result<Self> {
        let plan = native.plan().clone();
        let mut ordered = origins
            .drain(..)
            .map(|origin| Ok((origin.identity()?, origin)))
            .collect::<Result<Vec<_>>>()?;
        ordered.sort_by_key(|(identity, _)| *identity);
        let mut origins = ordered
            .into_iter()
            .map(|(_, origin)| origin)
            .collect::<Vec<_>>();
        let claims = origins
            .iter()
            .map(configuration_claim)
            .collect::<Result<Vec<_>>>()?;
        validate_origins(&plan, &mut origins, &claims)?;
        native.revalidate()?;
        let authority = NativeStandaloneAuthority::from_completed_adoption_claims(
            native,
            receipt,
            claims.clone(),
        )?;
        Ok(Self {
            origins,
            claims,
            authority,
            plan,
        })
    }
    pub(crate) fn import(&mut self, registry: &mut ServiceRegistry) -> Result<PersistedGrant> {
        let host = crate::catalog_host_context::current_native_host(registry.incarnation()?)?;
        let origins = &mut self.origins;
        let claims = &self.claims;
        let plan = &self.plan;
        // Store invokes this actual native check before effects and immediately
        // before committing both new and reconciled authority transactions.
        registry.import_standalone_authority_checked(&self.authority, host, &mut || {
            validate_origins(plan, origins, claims)
        })
    }
    pub(crate) fn into_origins(self) -> Vec<crate::runtime_legacy_origin::NativeLegacyOrigin> {
        self.origins
    }
}

fn validate_origins(
    plan: &LegacyAdoptionPlan,
    origins: &mut [crate::runtime_legacy_origin::NativeLegacyOrigin],
    claims: &[StandaloneConfigurationClaim],
) -> Result<()> {
    ensure!(
        !origins.is_empty() && origins.len() <= 32 && origins.len() == claims.len(),
        "standalone native claim roster changed"
    );
    let mut primary = false;
    for (origin, expected) in origins.iter_mut().zip(claims) {
        crate::catalog_index_setup::validate_legacy_adoption_origin_scope(plan, origin)?;
        let actual = configuration_claim(origin)?;
        ensure!(
            &actual == expected,
            "standalone native claim preimage changed"
        );
        if actual.origin_sha256 == plan.origin_sha256 {
            crate::catalog_index_setup::validate_legacy_adoption_origin(plan, origin)?;
            primary = true;
        }
    }
    ensure!(
        primary,
        "standalone primary origin is absent from held batch"
    );
    Ok(())
}
