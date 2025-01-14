// Copyright (C) 2019-2021 Aleo Systems Inc.
// This file is part of the snarkOS library.

// The snarkOS library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkOS library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkOS library. If not, see <https://www.gnu.org/licenses/>.

use tokio::time::sleep;

use crate::{
    consensus::{BLOCK_1, BLOCK_1_HEADER_HASH, BLOCK_2, BLOCK_2_HEADER_HASH, TRANSACTION_1, TRANSACTION_2},
    network::{handshaken_node_and_peer, test_node, ConsensusSetup, TestSetup},
    wait_until,
};

use snarkos_consensus::memory_pool::Entry;
use snarkos_network::message::*;

use snarkvm_dpc::instantiated::Tx;
use snarkvm_objects::block_header_hash::BlockHeaderHash;
#[cfg(test)]
use snarkvm_utilities::FromBytes;

use std::time::Duration;

#[tokio::test]
async fn block_initiator_side() {
    // handshake between a fake node and a full node
    let setup = TestSetup {
        consensus_setup: Some(ConsensusSetup {
            block_sync_interval: 1,
            ..Default::default()
        }),
        ..Default::default()
    };
    let (node, mut peer) = handshaken_node_and_peer(setup).await;

    // wait for the block_sync_interval to "expire"
    sleep(Duration::from_secs(1)).await;

    // trigger the full node to request synchronization by sending it a higher block_height than it has
    let ping = Payload::Ping(2u32);
    peer.write_message(&ping).await;

    // read the Pong
    let payload = peer.read_payload().await.unwrap();
    assert!(matches!(payload, Payload::Pong));

    // check if a GetSync message was received
    let payload = peer.read_payload().await.unwrap();
    assert!(matches!(payload, Payload::GetSync(..)));

    let block_1_header_hash = BlockHeaderHash::new(BLOCK_1_HEADER_HASH.to_vec());
    let block_2_header_hash = BlockHeaderHash::new(BLOCK_2_HEADER_HASH.to_vec());

    let block_header_hashes = vec![block_1_header_hash.clone(), block_2_header_hash.clone()];

    // respond to GetSync with Sync message containing the block header hashes of the missing
    // blocks
    let sync = Payload::Sync(block_header_hashes);
    peer.write_message(&sync).await;

    // make sure both GetBlock messages are received
    let payload = peer.read_payload().await.unwrap();
    let block_hashes = if let Payload::GetBlocks(block_hashes) = payload {
        block_hashes
    } else {
        unreachable!();
    };

    assert!(block_hashes.contains(&block_1_header_hash) && block_hashes.contains(&block_2_header_hash));

    // respond with the full blocks
    let block_1 = Payload::Block(BLOCK_1.to_vec());
    peer.write_message(&block_1).await;

    let block_2 = Payload::Block(BLOCK_2.to_vec());
    peer.write_message(&block_2).await;

    // check the blocks have been added to the node's chain
    wait_until!(
        1,
        node.expect_consensus()
            .storage()
            .block_hash_exists(&block_1_header_hash)
    );
    wait_until!(
        1,
        node.expect_consensus()
            .storage()
            .block_hash_exists(&block_2_header_hash)
    );
}

#[tokio::test]
async fn block_responder_side() {
    // handshake between a fake node and a full node
    let (node, mut peer) = handshaken_node_and_peer(TestSetup::default()).await;

    // insert block into node
    let block_struct_1 = snarkvm_objects::Block::deserialize(&BLOCK_1).unwrap();
    node.expect_consensus()
        .consensus_parameters()
        .receive_block(
            node.expect_consensus().dpc_parameters(),
            &node.expect_consensus().storage(),
            &mut node.expect_consensus().memory_pool().lock(),
            &block_struct_1,
        )
        .unwrap();

    // send a GetSync with an empty vec as only the genesis block is in the ledger
    let get_sync = Payload::GetSync(vec![]);
    peer.write_message(&get_sync).await;

    // receive a Sync message from the node with the block header
    let payload = peer.read_payload().await.unwrap();
    let sync = if let Payload::Sync(sync) = payload {
        sync
    } else {
        unreachable!();
    };

    let block_header_hash = sync.first().unwrap();

    // check it matches the block inserted into the node's ledger
    assert_eq!(*block_header_hash, block_struct_1.header.get_hash());

    // request the block from the node
    let get_block = Payload::GetBlocks(vec![block_header_hash.clone()]);
    peer.write_message(&get_block).await;

    // receive a SyncBlock message with the requested block
    let payload = peer.read_payload().await.unwrap();
    let block = if let Payload::SyncBlock(block) = payload {
        block
    } else {
        unreachable!();
    };
    let block = snarkvm_objects::Block::deserialize(&block).unwrap();

    assert_eq!(block, block_struct_1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn block_propagation() {
    let setup = TestSetup {
        consensus_setup: Some(ConsensusSetup {
            is_miner: true,
            ..Default::default()
        }),
        ..Default::default()
    };

    let (_node, mut peer) = handshaken_node_and_peer(setup).await;

    let payload = peer.read_payload().await.unwrap();
    assert!(matches!(payload, Payload::Block(..)));

    // TODO: shutdown the miner task, currently there is no good way to do this. This test will
    // currently hang after the assertion.
}

#[tokio::test]
#[ignore]
async fn block_two_node() {
    let setup = TestSetup {
        peer_sync_interval: 1,
        ..Default::default()
    };
    let node_alice = test_node(setup).await;
    let alice_address = node_alice.local_address().unwrap();

    const NUM_BLOCKS: usize = 100;

    let blocks = crate::network::TestBlocks::load(NUM_BLOCKS).0;
    assert_eq!(blocks.len(), NUM_BLOCKS);

    for block in blocks {
        node_alice
            .expect_consensus()
            .consensus_parameters()
            .receive_block(
                node_alice.expect_consensus().dpc_parameters(),
                &node_alice.expect_consensus().storage(),
                &mut node_alice.expect_consensus().memory_pool().lock(),
                &block,
            )
            .unwrap();
    }

    let setup = TestSetup {
        consensus_setup: Some(ConsensusSetup {
            block_sync_interval: 5,
            ..Default::default()
        }),
        peer_sync_interval: 5,
        bootnodes: vec![alice_address.to_string()],
        ..Default::default()
    };
    let node_bob = test_node(setup).await;

    // check blocks present in alice's chain were synced to bob's
    wait_until!(
        30,
        node_bob.expect_consensus().current_block_height() as usize == NUM_BLOCKS
    );
}

#[tokio::test]
async fn transaction_initiator_side() {
    // handshake between a fake node and a full node
    let setup = TestSetup {
        consensus_setup: Some(ConsensusSetup {
            tx_sync_interval: 1,
            ..Default::default()
        }),
        ..Default::default()
    };
    let (node, mut peer) = handshaken_node_and_peer(setup).await;

    // check GetMemoryPool message was received
    let payload = peer.read_payload().await.unwrap();
    assert!(matches!(payload, Payload::GetMemoryPool));

    // Respond with MemoryPool message
    let memory_pool = Payload::MemoryPool(vec![TRANSACTION_1.to_vec(), TRANSACTION_2.to_vec()]);
    peer.write_message(&memory_pool).await;

    // Create the entries to verify
    let entry_1 = Entry {
        size_in_bytes: TRANSACTION_1.len(),
        transaction: Tx::read(&TRANSACTION_1[..]).unwrap(),
    };

    let entry_2 = Entry {
        size_in_bytes: TRANSACTION_2.len(),
        transaction: Tx::read(&TRANSACTION_2[..]).unwrap(),
    };

    // Verify the transactions have been stored in the node's memory pool
    wait_until!(1, node.expect_consensus().memory_pool().lock().contains(&entry_1));
    wait_until!(1, node.expect_consensus().memory_pool().lock().contains(&entry_2));
}

#[tokio::test]
async fn transaction_responder_side() {
    // handshake between a fake node and a full node
    let (node, mut peer) = handshaken_node_and_peer(TestSetup::default()).await;

    // insert transaction into node
    let mut memory_pool = node.expect_consensus().memory_pool().lock();
    let storage = node.expect_consensus().storage();

    let entry_1 = Entry {
        size_in_bytes: TRANSACTION_1.len(),
        transaction: Tx::read(&TRANSACTION_1[..]).unwrap(),
    };

    let entry_2 = Entry {
        size_in_bytes: TRANSACTION_2.len(),
        transaction: Tx::read(&TRANSACTION_2[..]).unwrap(),
    };

    memory_pool.insert(&storage, entry_1).unwrap().unwrap();
    memory_pool.insert(&storage, entry_2).unwrap().unwrap();

    // drop the locks to avoid deadlocks
    drop(memory_pool);

    // send a GetMemoryPool message
    let get_memory_pool = Payload::GetMemoryPool;
    peer.write_message(&get_memory_pool).await;

    // check GetMemoryPool message was received
    let payload = peer.read_payload().await.unwrap();
    let txs = if let Payload::MemoryPool(txs) = payload {
        txs
    } else {
        unreachable!();
    };

    // check transactions
    assert!(txs.contains(&TRANSACTION_1.to_vec()));
    assert!(txs.contains(&TRANSACTION_2.to_vec()));
}

#[tokio::test]
async fn transaction_two_node() {
    use snarkos_consensus::memory_pool::Entry;
    use snarkvm_dpc::instantiated::Tx;
    use snarkvm_utilities::bytes::FromBytes;

    let node_alice = test_node(TestSetup::default()).await;
    let alice_address = node_alice.local_address().unwrap();

    // insert transaction into node_alice
    let mut memory_pool = node_alice.expect_consensus().memory_pool().lock();
    let storage = node_alice.expect_consensus().storage();

    let transaction = Tx::read(&TRANSACTION_1[..]).unwrap();
    let size = TRANSACTION_1.len();
    let entry = Entry {
        size_in_bytes: size,
        transaction: transaction.clone(),
    };

    memory_pool.insert(&storage, entry.clone()).unwrap().unwrap();

    // drop the locks to avoid deadlocks
    drop(memory_pool);

    let setup = TestSetup {
        consensus_setup: Some(ConsensusSetup {
            tx_sync_interval: 1,
            ..Default::default()
        }),
        peer_sync_interval: 1,
        bootnodes: vec![alice_address.to_string()],
        ..Default::default()
    };
    let node_bob = test_node(setup).await;

    // check transaction is present in bob's memory pool
    wait_until!(5, node_bob.expect_consensus().memory_pool().lock().contains(&entry));
}
