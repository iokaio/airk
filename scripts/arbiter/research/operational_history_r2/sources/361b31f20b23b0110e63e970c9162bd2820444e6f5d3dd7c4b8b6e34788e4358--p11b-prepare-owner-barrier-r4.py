# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json
root=Path('D:/code/Github/airk');prior=root/'artifacts/v190/p11b-owner-barrier-r3-proposed';out=root/'artifacts/v190/p11b-owner-barrier-r4-proposed';out.mkdir(exist_ok=True)
baseline=json.loads((prior/'baseline.json').read_text(encoding='utf-8'))
for path in baseline:
    dest=out/path;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text((prior/path).read_text(encoding='utf-8'),encoding='utf-8',newline='\n')
module=out/'crates/airk-store/src/service_collection_retention.rs';s=module.read_text(encoding='utf-8')
anchor='pub(super) fn install('
context='''/// Borrowed native ownership inputs, not serializable authority or a bypass.
pub(crate) struct RetentionOwnerContext<'a> {
    pub owning_store: &'a FileStore,
    pub writer: &'a crate::WriterLock,
    pub quota: &'a Quota,
    pub logical: &'a SetupDirectory,
    pub root: &'a str,
    pub collection: &'a str,
    pub contract: u32,
}

'''
assert s.count(anchor)==1;s=s.replace(anchor,context+anchor,1)
anchor='    let mut markers = Vec::new();'
replacement='''    let context = RetentionOwnerContext {
        owning_store: store, writer, quota, logical: &pin,
        root: &owner.root, collection: &owner.collection, contract,
    };
    let mut markers = Vec::new();'''
assert s.count(anchor)==1;s=s.replace(anchor,replacement,1)
old='''        let marker = candidate.prepare_extraction_contract(
            store,
            writer,
            quota,
            candidate_pin,
            &pin,
            &owner.root,
            &owner.collection,
            contract,
        )?;'''
assert s.count(old)==1;s=s.replace(old,'        let marker = candidate.prepare_extraction_contract(&context, candidate_pin)?;',1)
module.write_text(s,encoding='utf-8',newline='\n')
journal=out/'crates/airk-store/src/current/journal.rs';s=journal.read_text(encoding='utf-8');a=s.index('    pub(crate) fn prepare_extraction_contract(');b=s.index('        crate::maintenance::check_writer(owning_store, writer, quota)?;',a)
s=s[:a]+'''    pub(crate) fn prepare_extraction_contract(
        &self,
        context: &crate::service::collection::retention_barrier::RetentionOwnerContext<'_>,
        pin: crate::service::registry::catalog::setup::storage::SetupDirectory,
    ) -> Result<PreparedExtractionContract> {
        let owning_store = context.owning_store;
        let writer = context.writer;
        let quota = context.quota;
        let logical = context.logical;
        let root = context.root;
        let collection = context.collection;
        let contract = context.contract;
'''+s[b:]
s=s.replace('''        let mut prepared = PreparedExtractionContract {
            pin,''','''        let mut prepared = PreparedExtractionContract {
            directory: self.directory().to_path_buf(),
            pin,''',1)
s=s.replace('''pub(crate) struct PreparedExtractionContract {
    pin:''','''pub(crate) struct PreparedExtractionContract {
    directory: std::path::PathBuf,
    pin:''',1)
anchor='''    pub(crate) fn publish(&mut self) -> Result<()> {
'''
assert s.count(anchor)==1
s=s.replace(anchor,anchor+'''        self.require_named_directory()?;
''',1)
anchor='''impl PreparedExtractionContract {
'''
assert s.count(anchor)==1
s=s.replace(anchor,anchor+'''    fn require_named_directory(&self) -> Result<()> {
        ensure!(crate::service::registry::catalog::setup::storage::SetupDirectory::open_canonical(&self.directory)?.identity()?
            == self.pin.identity()?, "retained slot pathname changed during capability handoff");
        Ok(())
    }
''',1)
# The final operation in this exact owning impl must bracket the named slot,
# without using a reopened pathname as the publication destination.
a=s.index('impl PreparedExtractionContract {');sub=s[a:]
needle='''        Ok(())
    }
}'''
assert sub.count(needle)==1
b=sub.rindex(needle)
sub=sub[:b]+sub[b:].replace(needle,'''        self.require_named_directory()?;
        Ok(())
    }
}''',1)
s=s[:a]+sub
journal.write_text(s,encoding='utf-8',newline='\n')
(out/'baseline.json').write_text(json.dumps(baseline,indent=2)+'\n',encoding='utf-8')
print('prepared inert r4 private context; guards unchanged')
