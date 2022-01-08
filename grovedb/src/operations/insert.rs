use std::rc::Rc;

use storage::rocksdb_storage;

use crate::{Element, Error, GroveDb, Merk, PrefixedRocksDbStorage};

/// A helper function that builds a prefix for a key under a path and opens a
/// Merk instance.
fn create_merk_with_prefix(
    db: Rc<rocksdb_storage::DB>,
    path: &[&[u8]],
    key: &[u8],
) -> Result<(Vec<u8>, Merk<PrefixedRocksDbStorage>), Error> {
    let subtree_prefix = GroveDb::compress_subtree_key(&path, Some(&key));
    Ok((
        subtree_prefix.clone(),
        Merk::open(PrefixedRocksDbStorage::new(db, subtree_prefix)?)
            .map_err(|e| Error::CorruptedData(e.to_string()))?,
    ))
}

impl GroveDb {
    pub fn insert(&mut self, path: &[&[u8]], key: Vec<u8>, element: Element) -> Result<(), Error> {
        match element {
            Element::Tree(_) => {
                if path.is_empty() {
                    self.add_root_leaf(&key)?;
                } else {
                    self.add_non_root_subtree(path, key)?;
                }
                self.store_subtrees_keys_data()?;
            }
            _ => {
                // If path is empty that means there is an attempt to insert something into a
                // root tree and this branch is for anything but trees
                if path.is_empty() {
                    return Err(Error::InvalidPath(
                        "only subtrees are allowed as root tree's leafs",
                    ));
                }
                // Get a Merk by a path
                let mut merk = self
                    .subtrees
                    .get_mut(&Self::compress_subtree_key(path, None))
                    .ok_or(Error::InvalidPath("no subtree found under that path"))?;
                element.insert(&mut merk, key)?;
                self.propagate_changes(path)?;
            }
        }
        Ok(())
    }

    /// Add subtree to the root tree
    fn add_root_leaf(&mut self, key: &[u8]) -> Result<(), Error> {
        // Open Merk and put handle into `subtrees` dictionary accessible by its
        // compressed path
        let (subtree_prefix, subtree_merk) = create_merk_with_prefix(self.db.clone(), &[], &key)?;
        self.subtrees.insert(subtree_prefix.clone(), subtree_merk);

        // Update root leafs index to persist rs-merkle structure later
        if self.root_leaf_keys.get(&subtree_prefix).is_none() {
            self.root_leaf_keys
                .insert(subtree_prefix, self.root_tree.leaves_len());
        }
        self.propagate_changes(&[&key])?;
        Ok(())
    }

    // Add subtree to another subtree.
    fn add_non_root_subtree(&mut self, path: &[&[u8]], key: Vec<u8>) -> Result<(), Error> {
        let compressed_path = Self::compress_subtree_key(path, None);
        // First, check if a subtree exists to create a new subtree under it
        self.subtrees
            .get(&compressed_path)
            .ok_or(Error::InvalidPath("no subtree found under that path"))?;
        let (subtree_prefix, subtree_merk) = create_merk_with_prefix(self.db.clone(), path, &key)?;
        // Set tree value as a a subtree root hash
        let element = Element::Tree(subtree_merk.root_hash());
        self.subtrees.insert(subtree_prefix, subtree_merk);
        // Had to take merk from `subtrees` once again to solve multiple &mut s
        let mut merk = self
            .subtrees
            .get_mut(&compressed_path)
            .expect("merk object must exist in `subtrees`");
        // need to mark key as taken in the upper tree
        element.insert(&mut merk, key)?;
        self.propagate_changes(path)?;
        Ok(())
    }

    pub fn insert_if_not_exists(
        &mut self,
        path: &[&[u8]],
        key: Vec<u8>,
        element: Element,
    ) -> Result<bool, Error> {
        if self.get(path, &key).is_ok() {
            return Ok(false);
        }
        match self.insert(path, key, element) {
            Ok(_) => Ok(true),
            Err(e) => Err(e),
        }
    }
}