use storage::{rocksdb_storage::OptimisticTransactionDBTransaction, RawIterator};

use crate::{Error, GroveDb};

impl GroveDb {
    pub fn is_empty_tree(
        &self,
        path: &[&[u8]],
        transaction: Option<&OptimisticTransactionDBTransaction>,
    ) -> Result<bool, Error> {
        let merk = self.get_subtrees().get_subtree(path, transaction)?;

        let mut iter = merk.raw_iter();
        iter.seek_to_first();

        Ok(!iter.valid())
    }
}
