// SPDX-License-Identifier: Apache-2.0
//! The legacy-binding registration reader. The imported-grant and adopted
//! standalone snapshot readers were removed with reviewed grants (change-log
//! entry 68 part 3).
use airk_core::service::{ProjectBinding, ProjectRegistration};
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;

impl super::super::ServiceRegistry {
    /// Exact registered metadata only, with no filesystem inspection or claim.
    /// A missing row is a logical absence; SQLite/codec errors remain errors.
    pub fn legacy_binding_registration(
        &self,
        binding: ProjectBinding,
    ) -> Result<Option<ProjectRegistration>> {
        let row=self.connection.query_row(
            if self.registration_state {"SELECT substr(payload,1,20481),epoch,paused FROM projects WHERE binding=?1 AND registered=1"}
            else {"SELECT substr(payload,1,20481),epoch,paused FROM projects WHERE binding=?1"},
            [binding.to_string()],|row|Ok((row.get::<_,Vec<u8>>(0)?,row.get::<_,i64>(1)?,row.get::<_,bool>(2)?)),
        ).optional()?;
        row.map(|(payload, epoch, paused)| {
            let project = crate::service::registry::decode(&payload, epoch, paused)?;
            ensure!(
                project.registration.binding == binding,
                "foreign native legacy registration"
            );
            Ok(project.registration)
        })
        .transpose()
    }
}
