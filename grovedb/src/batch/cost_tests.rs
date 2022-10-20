#[cfg(test)]
mod tests {
    use std::option::Option::None;

    use costs::{
        storage_cost::{
            removal::StorageRemovedBytes::{BasicStorageRemoval, NoStorageRemoval},
            transition::OperationStorageTransitionType,
            StorageCost,
        },
        OperationCost,
    };
    use integer_encoding::VarInt;
    use merk::proofs::Query;

    use super::*;
    use crate::{
        batch::GroveDbOp,
        reference_path::ReferencePathType,
        tests::{make_empty_grovedb, make_test_grovedb, ANOTHER_TEST_LEAF, TEST_LEAF},
        Element, PathQuery,
    };

    #[test]
    fn test_batch_costs_match_non_batch() {
        let db = make_empty_grovedb();
        let tx = db.start_transaction();

        let non_batch_cost = db
            .insert(vec![], b"key1", Element::empty_tree(), None, Some(&tx))
            .cost;
        tx.rollback().expect("expected to rollback");
        let ops = vec![GroveDbOp::insert_run_op(
            vec![],
            b"key1".to_vec(),
            Element::empty_tree(),
        )];
        let cost = db.apply_batch(ops, None, Some(&tx)).cost;
        assert_eq!(non_batch_cost.storage_cost, cost.storage_cost);
    }

    #[test]
    fn test_batch_root_one_insert_tree_cost() {
        let db = make_empty_grovedb();
        let tx = db.start_transaction();

        let ops = vec![GroveDbOp::insert_run_op(
            vec![],
            b"key1".to_vec(),
            Element::empty_tree(),
        )];
        let cost_result = db.apply_batch(ops, None, Some(&tx));
        cost_result.value.expect("expected to execute batch");
        let cost = cost_result.cost;
        // Explanation for 214 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 68
        //   1 for the flag option (but no flags)
        //   1 for the enum type
        //   1 for empty tree value
        // 32 for node hash
        // 32 for value hash
        // 1 byte for the value_size (required space for 98)

        // Parent Hook -> 39
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2

        // Total 37 + 68 + 39 = 144

        // Hash node calls
        // 2 for the node hash
        // 1 for the value hash
        assert_eq!(
            cost,
            OperationCost {
                seek_count: 2, // 1 to get tree, 1 to insert
                storage_cost: StorageCost {
                    added_bytes: 144,
                    replaced_bytes: 0,
                    removed_bytes: NoStorageRemoval,
                },
                storage_loaded_bytes: 0,
                hash_node_calls: 6,
            }
        );
    }

    #[test]
    fn test_batch_root_one_insert_item_cost() {
        let db = make_empty_grovedb();
        let tx = db.start_transaction();

        let ops = vec![GroveDbOp::insert_run_op(
            vec![],
            b"key1".to_vec(),
            Element::new_item([0u8; 32].to_vec()),
        )];
        let cost_result = db.apply_batch(ops, None, Some(&tx));
        cost_result.value.expect("expected to execute batch");
        let cost = cost_result.cost;
        // Explanation for 214 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 100
        //   1 for the flag option (but no flags)
        //   1 for the enum type
        //   1 for required space for bytes
        //   32 for item bytes
        // 32 for node hash
        // 32 for value hash
        // 1 byte for the value_size (required space for 99)

        // Parent Hook -> 39
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2

        // Root -> 39
        // 1 byte for the root key length size
        // 1 byte for the root value length size
        // 32 for the root key prefix
        // 4 bytes for the key to put in root
        // 1 byte for the root "r"

        // Total 37 + 99 + 39 = 175

        // Hash node calls
        // 2 for the node hash
        // 1 for the value hash
        assert_eq!(
            cost,
            OperationCost {
                seek_count: 2, // 1 to get tree, 1 to insert
                storage_cost: StorageCost {
                    added_bytes: 175,
                    replaced_bytes: 0,
                    removed_bytes: NoStorageRemoval,
                },
                storage_loaded_bytes: 0,
                hash_node_calls: 6,
            }
        );
    }

    #[test]
    fn test_batch_root_one_insert_cost_right_below_value_required_cost_of_2() {
        let db = make_empty_grovedb();
        let tx = db.start_transaction();

        let ops = vec![GroveDbOp::insert_run_op(
            vec![],
            b"key1".to_vec(),
            Element::new_item([0u8; 60].to_vec()),
        )];
        let cost_result = db.apply_batch(ops, None, Some(&tx));
        cost_result.value.expect("expected to execute batch");
        let cost = cost_result.cost;
        // Explanation for 243 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 128
        //   1 for the flag option (but no flags)
        //   1 for the enum type
        //   1 for the value size
        //   60 bytes
        // 32 for node hash
        // 32 for value hash
        // 1 byte for the value_size (required space for 127)

        // Parent Hook -> 39
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2

        // Total 37 + 128 + 39 = 204

        // Hash node calls
        // 2 for the node hash
        // 1 for the value hash
        assert_eq!(
            cost,
            OperationCost {
                seek_count: 2, // 1 to insert, 1 for root tree.
                storage_cost: StorageCost {
                    added_bytes: 204,
                    replaced_bytes: 0,
                    removed_bytes: NoStorageRemoval,
                },
                storage_loaded_bytes: 0,
                hash_node_calls: 6,
            }
        );
    }

    #[test]
    fn test_batch_root_one_insert_cost_right_above_value_required_cost_of_2() {
        let db = make_empty_grovedb();
        let tx = db.start_transaction();

        let ops = vec![GroveDbOp::insert_run_op(
            vec![],
            b"key1".to_vec(),
            Element::new_item([0u8; 61].to_vec()),
        )];
        let cost_result = db.apply_batch(ops, None, Some(&tx));
        cost_result.value.expect("expected to execute batch");
        let cost = cost_result.cost;
        // Explanation for 243 storage_written_bytes

        // Key -> 37 bytes
        // 32 bytes for the key prefix
        // 4 bytes for the key
        // 1 byte for key_size (required space for 36)

        // Value -> 130
        //   1 for the flag option (but no flags)
        //   1 for the enum type
        //   1 for the value size
        //   61 bytes
        // 32 for node hash
        // 32 for value hash
        // 2 byte for the value_size (required space for 128)

        // Parent Hook -> 39
        // Key Bytes 4
        // Hash Size 32
        // Key Length 1
        // Child Heights 2

        // Total 37 + 128 + 39 = 204

        // Hash node calls
        // 2 for the node hash
        // 1 for the value hash
        assert_eq!(
            cost,
            OperationCost {
                seek_count: 2, // 1 to insert, 1 for insert to root tree
                storage_cost: StorageCost {
                    added_bytes: 204,
                    replaced_bytes: 0,
                    removed_bytes: NoStorageRemoval,
                },
                storage_loaded_bytes: 0,
                hash_node_calls: 6, // todo: explain this
            }
        );
    }

    #[test]
    fn test_batch_root_one_update_bigger_cost_no_flags() {
        let db = make_empty_grovedb();
        let tx = db.start_transaction();
        db.insert(vec![], b"tree", Element::empty_tree(), None, None)
            .unwrap()
            .expect("expected to insert tree");

        db.insert(
            vec![b"tree".as_slice()],
            b"key1",
            Element::new_item_with_flags(b"value1".to_vec(), Some(vec![0])),
            None,
            None,
        )
        .unwrap()
        .expect("expected to insert item");

        // We are adding 2 bytes
        let ops = vec![GroveDbOp::insert_run_op(
            vec![b"tree".to_vec()],
            b"key1".to_vec(),
            Element::new_item_with_flags(b"value100".to_vec(), Some(vec![1])),
        )];

        let cost = db
            .apply_batch_with_element_flags_update(
                ops,
                None,
                |_cost, _old_flags, _new_flags| Ok(false),
                |_flags, _removed_bytes| Ok(NoStorageRemoval),
                Some(&tx),
            )
            .cost;

        assert_eq!(
            cost,
            OperationCost {
                seek_count: 6, // 1 to get tree, 1 to insert
                storage_cost: StorageCost {
                    added_bytes: 2,
                    replaced_bytes: 257,
                    removed_bytes: NoStorageRemoval
                },
                storage_loaded_bytes: 185,
                hash_node_calls: 6,
            }
        );
    }

    #[test]
    fn test_batch_root_one_update_bigger_cost() {
        let db = make_empty_grovedb();
        let tx = db.start_transaction();
        db.insert(vec![], b"tree", Element::empty_tree(), None, None)
            .unwrap()
            .expect("expected to insert tree");

        db.insert(
            vec![b"tree".as_slice()],
            b"key1",
            Element::new_item_with_flags(b"value1".to_vec(), Some(vec![0, 0])),
            None,
            None,
        )
        .unwrap()
        .expect("expected to insert item");

        // We are adding 2 bytes
        let ops = vec![GroveDbOp::insert_run_op(
            vec![b"tree".to_vec()],
            b"key1".to_vec(),
            Element::new_item_with_flags(b"value100".to_vec(), Some(vec![0, 1])),
        )];

        let cost = db
            .apply_batch_with_element_flags_update(
                ops,
                None,
                |cost, old_flags, new_flags| match cost.transition_type() {
                    OperationStorageTransitionType::OperationUpdateBiggerSize => {
                        if new_flags[0] == 0 {
                            new_flags[0] = 1;
                            let new_flags_epoch = new_flags[1];
                            new_flags[1] = old_flags.unwrap()[1];
                            new_flags.push(new_flags_epoch);
                            new_flags.extend(cost.added_bytes.encode_var_vec());
                            assert_eq!(new_flags, &vec![1u8, 0, 1, 2]);
                            Ok(true)
                        } else {
                            assert_eq!(new_flags[0], 1);
                            Ok(false)
                        }
                    }
                    OperationStorageTransitionType::OperationUpdateSmallerSize => {
                        new_flags.extend(vec![1, 2]);
                        Ok(true)
                    }
                    _ => Ok(false),
                },
                |_flags, removed| Ok(BasicStorageRemoval(removed)),
                Some(&tx),
            )
            .cost;

        assert_eq!(
            cost,
            OperationCost {
                seek_count: 4, // todo: verify this
                storage_cost: StorageCost {
                    added_bytes: 4,
                    replaced_bytes: 258,
                    removed_bytes: NoStorageRemoval
                },
                storage_loaded_bytes: 186, // todo: verify this
                hash_node_calls: 6,        // todo: verify this
            }
        );
    }
}