use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt};
use ed::{Decode, Encode, Result, Terminated};

use super::{hash::Hash, Tree};

// TODO: optimize memory footprint

/// Represents a reference to a child tree node. Links may or may not contain
/// the child's `Tree` instance (storing its key if not).
#[derive(Clone)]
pub enum Link {
    /// Represents a child tree node which has been pruned from memory, only
    /// retaining a reference to it (its key). The child node can always be
    /// fetched from the backing store by this key when necessary.
    Reference {
        hash: Hash,
        child_heights: (u8, u8),
        key: Vec<u8>,
        sum: Option<u64>,
    },

    /// Represents a tree node which has been modified since the `Tree`'s last
    /// hash computation. The child's hash is not stored since it has not yet
    /// been recomputed. The child's `Tree` instance is stored in the link.
    #[rustfmt::skip]
    Modified {
        pending_writes: usize, // TODO: rename to `pending_hashes`
        child_heights: (u8, u8),
        tree: Tree
    },

    /// Represents a tree node which has been modified since the `Tree`'s last
    /// commit, but which has an up-to-date hash. The child's `Tree` instance is
    /// stored in the link.
    Uncommitted {
        hash: Hash,
        child_heights: (u8, u8),
        tree: Tree,
        sum: Option<u64>,
    },

    /// Represents a tree node which has not been modified, has an up-to-date
    /// hash, and which is being retained in memory.
    Loaded {
        hash: Hash,
        child_heights: (u8, u8),
        tree: Tree,
        sum: Option<u64>,
    },
}

impl Link {
    /// Creates a `Link::Modified` from the given `Tree`.
    #[inline]
    pub const fn from_modified_tree(tree: Tree) -> Self {
        let pending_writes = 1 + tree.child_pending_writes(true) + tree.child_pending_writes(false);

        Self::Modified {
            pending_writes,
            child_heights: tree.child_heights(),
            tree,
        }
    }

    /// Creates a `Link::Modified` from the given tree, if any. If `None`,
    /// returns `None`.
    pub fn maybe_from_modified_tree(maybe_tree: Option<Tree>) -> Option<Self> {
        maybe_tree.map(Self::from_modified_tree)
    }

    /// Returns `true` if the link is of the `Link::Reference` variant.
    #[inline]
    pub const fn is_reference(&self) -> bool {
        matches!(self, Link::Reference { .. })
    }

    /// Returns `true` if the link is of the `Link::Modified` variant.
    #[inline]
    pub const fn is_modified(&self) -> bool {
        matches!(self, Link::Modified { .. })
    }

    /// Returns `true` if the link is of the `Link::Uncommitted` variant.
    #[inline]
    pub const fn is_uncommitted(&self) -> bool {
        matches!(self, Link::Uncommitted { .. })
    }

    /// Returns `true` if the link is of the `Link::Loaded` variant.
    #[inline]
    pub const fn is_stored(&self) -> bool {
        matches!(self, Link::Loaded { .. })
    }

    /// Returns the key of the tree referenced by this link, as a slice.
    #[inline]
    pub fn key(&self) -> &[u8] {
        match self {
            Link::Reference { key, .. } => key.as_slice(),
            Link::Modified { tree, .. } => tree.key(),
            Link::Uncommitted { tree, .. } => tree.key(),
            Link::Loaded { tree, .. } => tree.key(),
        }
    }

    /// Returns the `Tree` instance of the tree referenced by the link. If the
    /// link is of variant `Link::Reference`, the returned value will be `None`.
    #[inline]
    pub const fn tree(&self) -> Option<&Tree> {
        match self {
            // TODO: panic for Reference, don't return Option?
            Link::Reference { .. } => None,
            Link::Modified { tree, .. } => Some(tree),
            Link::Uncommitted { tree, .. } => Some(tree),
            Link::Loaded { tree, .. } => Some(tree),
        }
    }

    /// Returns the hash of the tree referenced by the link. Panics if link is
    /// of variant `Link::Modified` since we have not yet recomputed the tree's
    /// hash.
    #[inline]
    pub const fn hash(&self) -> &Hash {
        match self {
            Link::Modified { .. } => panic!("Cannot get hash from modified link"),
            Link::Reference { hash, .. } => hash,
            Link::Uncommitted { hash, .. } => hash,
            Link::Loaded { hash, .. } => hash,
        }
    }

    /// Returns the sum of the tree referenced by the link. Panics if link is
    /// of variant `Link::Modified` since we have not yet recomputed the tree's
    /// hash.
    #[inline]
    pub const fn sum(&self) -> Option<u64> {
        match self {
            Link::Modified { .. } => panic!("Cannot get hash from modified link"),
            Link::Reference { sum, .. } => *sum,
            Link::Uncommitted { sum, .. } => *sum,
            Link::Loaded { sum, .. } => *sum,
        }
    }

    /// Returns the height of the children of the tree referenced by the link,
    /// if any (note: not the height of the referenced tree itself). Return
    /// value is `(left_child_height, right_child_height)`.
    #[inline]
    pub const fn height(&self) -> u8 {
        const fn max(a: u8, b: u8) -> u8 {
            if a >= b {
                a
            } else {
                b
            }
        }

        let (left_height, right_height) = match self {
            Link::Reference { child_heights, .. } => *child_heights,
            Link::Modified { child_heights, .. } => *child_heights,
            Link::Uncommitted { child_heights, .. } => *child_heights,
            Link::Loaded { child_heights, .. } => *child_heights,
        };
        1 + max(left_height, right_height)
    }

    /// Returns the balance factor of the tree referenced by the link.
    #[inline]
    pub const fn balance_factor(&self) -> i8 {
        let (left_height, right_height) = match self {
            Link::Reference { child_heights, .. } => *child_heights,
            Link::Modified { child_heights, .. } => *child_heights,
            Link::Uncommitted { child_heights, .. } => *child_heights,
            Link::Loaded { child_heights, .. } => *child_heights,
        };
        right_height as i8 - left_height as i8
    }

    /// Consumes the link and converts to variant `Link::Reference`. Panics if
    /// the link is of variant `Link::Modified` or `Link::Uncommitted`.
    #[inline]
    pub fn into_reference(self) -> Self {
        match self {
            Link::Reference { .. } => self,
            Link::Modified { .. } => panic!("Cannot prune Modified tree"),
            Link::Uncommitted { .. } => panic!("Cannot prune Uncommitted tree"),
            Link::Loaded {
                hash,
                sum,
                child_heights,
                tree,
            } => Self::Reference {
                hash,
                sum,
                child_heights,
                key: tree.take_key(),
            },
        }
    }

    #[inline]
    pub(crate) fn child_heights_mut(&mut self) -> &mut (u8, u8) {
        match self {
            Link::Reference {
                ref mut child_heights,
                ..
            } => child_heights,
            Link::Modified {
                ref mut child_heights,
                ..
            } => child_heights,
            Link::Uncommitted {
                ref mut child_heights,
                ..
            } => child_heights,
            Link::Loaded {
                ref mut child_heights,
                ..
            } => child_heights,
        }
    }
}

impl Encode for Link {
    #[inline]
    fn encode_into<W: Write>(&self, out: &mut W) -> Result<()> {
        let (hash, sum, key, (left_height, right_height)) = match self {
            Link::Reference {
                hash,
                sum,
                key,
                child_heights,
            } => (hash, sum, key.as_slice(), child_heights),
            Link::Loaded {
                hash,
                sum,
                tree,
                child_heights,
            } => (hash, sum, tree.key(), child_heights),
            Link::Uncommitted {
                hash,
                sum,
                tree,
                child_heights,
            } => (hash, sum, tree.key(), child_heights),

            Link::Modified { .. } => panic!("No encoding for Link::Modified"),
        };

        debug_assert!(key.len() < 256, "Key length must be less than 256");

        out.write_all(&[key.len() as u8])?;
        out.write_all(key)?;

        out.write_all(hash)?;

        out.write_all(&[*left_height, *right_height])?;

        out.write_all(&[sum.is_some() as u8])?;
        if let Some(sum) = sum {
            out.write_all(sum.to_be_bytes().as_slice())?;
        }

        Ok(())
    }

    #[inline]
    fn encoding_length(&self) -> Result<usize> {
        debug_assert!(self.key().len() < 256, "Key length must be less than 256");

        Ok(match self {
            Link::Reference { key, sum, .. } => {
                1 + key.len() + 32 + 2 + 1 + (sum.is_some() as usize * 8)
            }
            Link::Modified { .. } => panic!("No encoding for Link::Modified"),
            Link::Uncommitted { tree, sum, .. } => {
                1 + tree.key().len() + 32 + 2 + 1 + (sum.is_some() as usize * 8)
            }
            Link::Loaded { tree, sum, .. } => {
                1 + tree.key().len() + 32 + 2 + 1 + (sum.is_some() as usize * 8)
            }
        })
    }
}

impl Link {
    #[inline]
    fn default_reference() -> Self {
        Self::Reference {
            key: Vec::with_capacity(64),
            hash: Default::default(),
            sum: None,
            child_heights: (0, 0),
        }
    }
}

impl Decode for Link {
    #[inline]
    fn decode<R: Read>(input: R) -> Result<Self> {
        let mut link = Self::default_reference();
        Self::decode_into(&mut link, input)?;
        Ok(link)
    }

    #[inline]
    fn decode_into<R: Read>(&mut self, mut input: R) -> Result<()> {
        if !self.is_reference() {
            // don't create new struct if self is already Link::Reference,
            // so we can re-use the key vec
            *self = Self::default_reference();
        }

        if let Link::Reference {
            ref mut sum,
            ref mut key,
            ref mut hash,
            ref mut child_heights,
        } = self
        {
            let length = read_u8(&mut input)? as usize;

            key.resize(length, 0);
            input.read_exact(key.as_mut())?;

            input.read_exact(&mut hash[..])?;

            child_heights.0 = read_u8(&mut input)?;
            child_heights.1 = read_u8(&mut input)?;

            let has_sum = input.read_u8()? != 0;
            *sum = if has_sum {
                Some(input.read_u64::<BigEndian>()?)
            } else {
                None
            };
        } else {
            unreachable!()
        }

        Ok(())
    }
}

impl Terminated for Link {}

#[inline]
fn read_u8<R: Read>(mut input: R) -> Result<u8> {
    let mut length = [0];
    input.read_exact(length.as_mut())?;
    Ok(length[0])
}

#[cfg(test)]
mod test {
    use super::{
        super::{hash::NULL_HASH, Tree},
        *,
    };
    use crate::merk::tree_feature_type::TreeFeatureType::BasicMerk;

    #[test]
    fn from_modified_tree() {
        let tree = Tree::new(vec![0], vec![1], BasicMerk).unwrap();
        let link = Link::from_modified_tree(tree);
        assert!(link.is_modified());
        assert_eq!(link.height(), 1);
        assert_eq!(link.tree().expect("expected tree").key(), &[0]);
        if let Link::Modified { pending_writes, .. } = link {
            assert_eq!(pending_writes, 1);
        } else {
            panic!("Expected Link::Modified");
        }
    }

    #[test]
    fn maybe_from_modified_tree() {
        let link = Link::maybe_from_modified_tree(None);
        assert!(link.is_none());

        let tree = Tree::new(vec![0], vec![1], BasicMerk).unwrap();
        let link = Link::maybe_from_modified_tree(Some(tree));
        assert!(link.expect("expected link").is_modified());
    }

    #[test]
    fn types() {
        let hash = NULL_HASH;
        let sum = None;
        let child_heights = (0, 0);
        let pending_writes = 1;
        let key = vec![0];
        let tree = || Tree::new(vec![0], vec![1], BasicMerk).unwrap();

        let reference = Link::Reference {
            hash,
            sum,
            child_heights,
            key,
        };
        let modified = Link::Modified {
            pending_writes,
            child_heights,
            tree: tree(),
        };
        let uncommitted = Link::Uncommitted {
            hash,
            sum,
            child_heights,
            tree: tree(),
        };
        let loaded = Link::Loaded {
            hash,
            sum,
            child_heights,
            tree: tree(),
        };

        assert!(reference.is_reference());
        assert!(!reference.is_modified());
        assert!(!reference.is_uncommitted());
        assert!(!reference.is_stored());
        assert!(reference.tree().is_none());
        assert_eq!(reference.hash(), &[0; 32]);
        assert_eq!(reference.height(), 1);
        assert!(reference.into_reference().is_reference());

        assert!(!modified.is_reference());
        assert!(modified.is_modified());
        assert!(!modified.is_uncommitted());
        assert!(!modified.is_stored());
        assert!(modified.tree().is_some());
        assert_eq!(modified.height(), 1);

        assert!(!uncommitted.is_reference());
        assert!(!uncommitted.is_modified());
        assert!(uncommitted.is_uncommitted());
        assert!(!uncommitted.is_stored());
        assert!(uncommitted.tree().is_some());
        assert_eq!(uncommitted.hash(), &[0; 32]);
        assert_eq!(uncommitted.height(), 1);

        assert!(!loaded.is_reference());
        assert!(!loaded.is_modified());
        assert!(!loaded.is_uncommitted());
        assert!(loaded.is_stored());
        assert!(loaded.tree().is_some());
        assert_eq!(loaded.hash(), &[0; 32]);
        assert_eq!(loaded.height(), 1);
        assert!(loaded.into_reference().is_reference());
    }

    #[test]
    #[should_panic]
    fn modified_hash() {
        Link::Modified {
            pending_writes: 1,
            child_heights: (1, 1),
            tree: Tree::new(vec![0], vec![1], BasicMerk).unwrap(),
        }
        .hash();
    }

    #[test]
    #[should_panic]
    fn modified_into_reference() {
        Link::Modified {
            pending_writes: 1,
            child_heights: (1, 1),
            tree: Tree::new(vec![0], vec![1], BasicMerk).unwrap(),
        }
        .into_reference();
    }

    #[test]
    #[should_panic]
    fn uncommitted_into_reference() {
        Link::Uncommitted {
            hash: [1; 32],
            sum: None,
            child_heights: (1, 1),
            tree: Tree::new(vec![0], vec![1], BasicMerk).unwrap(),
        }
        .into_reference();
    }

    #[test]
    fn encode_link() {
        let link = Link::Reference {
            key: vec![1, 2, 3],
            sum: None,
            child_heights: (123, 124),
            hash: [55; 32],
        };
        assert_eq!(link.encoding_length().unwrap(), 39);

        let mut bytes = vec![];
        link.encode_into(&mut bytes).unwrap();
        assert_eq!(
            bytes,
            vec![
                3, 1, 2, 3, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55,
                55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 123, 124, 0
            ]
        );
    }

    #[test]
    fn encode_link_with_sum() {
        let link = Link::Reference {
            key: vec![1, 2, 3],
            sum: Some(50),
            child_heights: (123, 124),
            hash: [55; 32],
        };
        assert_eq!(link.encoding_length().unwrap(), 47);

        let mut bytes = vec![];
        link.encode_into(&mut bytes).unwrap();
        assert_eq!(
            bytes,
            vec![
                3, 1, 2, 3, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55,
                55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 123, 124, 1, 0, 0, 0, 0, 0,
                0, 0, 50
            ]
        );
    }

    #[test]
    #[should_panic]
    fn encode_link_long_key() {
        let link = Link::Reference {
            key: vec![123; 300],
            sum: None,
            child_heights: (123, 124),
            hash: [55; 32],
        };
        let mut bytes = vec![];
        link.encode_into(&mut bytes).unwrap();
    }

    #[test]
    fn decode_link() {
        let bytes = vec![
            3, 1, 2, 3, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55,
            55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 55, 123, 124, 0,
        ];
        let link = Link::decode(bytes.as_slice()).expect("expected to decode a link");
        assert_eq!(link.sum(), None);
    }
}
