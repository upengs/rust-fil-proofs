use anyhow::{ensure, Context, Result};
use ff::Field;
use generic_array::typenum::Unsigned;
use itertools::Itertools;
use merkletree::{merkle::get_merkle_tree_len, store::StoreConfig};
use rayon::prelude::*;
use sha2raw::Sha256;
use storage_proofs_core::{
    cache_key::CacheKey,
    hasher::{Domain, Hasher},
    merkle::{MerkleTreeTrait, OctLCMerkleTree},
    util::NODE_SIZE,
};

use super::{
    batch_hasher::{batch_hash, truncate_hash},
    butterfly_graph::ButterflyGraph,
    expander_graph::ExpanderGraph,
    Config,
};
use crate::encode;

/// Encodes the provided data and returns the replica and a list of merkle trees for each layer.
pub fn encode_with_trees<H: 'static + Hasher>(
    config: &Config,
    store_config: StoreConfig,
    window_index: u32,
    replica_id: &H::Domain,
    data: &[u8],
) -> Result<(Vec<u8>, Vec<OctLCMerkleTree<H>>)> {
    let num_layers = config.num_layers();
    let num_leafs = config.n / NODE_SIZE;
    let mut trees = Vec::with_capacity(num_layers);
    let tree_len = Some(get_merkle_tree_len(
        num_leafs,
        <OctLCMerkleTree<H> as MerkleTreeTrait>::Arity::to_usize(),
    )?);

    let mut previous_layer = vec![0u8; config.n];
    let mut current_layer = vec![0u8; config.n];

    // 1. Construct the mask
    const MASK_LAYER_INDEX: u32 = 1;
    mask_layer(config, window_index, replica_id, &mut previous_layer)
        .context("failed to construct the mask layer")?;
    let mask_config = StoreConfig::from_config(
        &store_config,
        CacheKey::label_layer_with_window(MASK_LAYER_INDEX, window_index),
        tree_len,
    );
    let mask_tree = lc_tree_from_slice(&previous_layer, mask_config)
        .context("failed to construct merkle tree for the mask layer")?;
    trees.push(mask_tree);

    // 2. Construct expander layers
    for layer_index in 2..=(config.num_expander_layers as u32) {
        expander_layer(
            config,
            window_index,
            replica_id,
            layer_index,
            &previous_layer,
            &mut current_layer,
        )
        .context("failed to construct expander layer")?;

        let store_config = StoreConfig::from_config(
            &store_config,
            CacheKey::label_layer_with_window(layer_index, window_index),
            tree_len,
        );
        let tree = lc_tree_from_slice(&current_layer, store_config)
            .context("failed to construct merkle tree for expander layer")?;
        trees.push(tree);

        // swap layers to reuse memory
        std::mem::swap(&mut previous_layer, &mut current_layer);
    }

    // 3. Construct butterfly layers
    for layer_index in (1 + config.num_expander_layers as u32)..(num_layers as u32) {
        butterfly_layer(
            config,
            window_index,
            replica_id,
            layer_index,
            &previous_layer,
            &mut current_layer,
        )
        .context("failed to construct butterfly layer")?;

        let store_config = StoreConfig::from_config(
            &store_config,
            CacheKey::label_layer_with_window(layer_index, window_index),
            tree_len,
        );
        let tree = lc_tree_from_slice(&current_layer, store_config)
            .context("failed to construct merkle tree for butterfly layer")?;
        trees.push(tree);

        // swap layers to reuse memory
        std::mem::swap(&mut previous_layer, &mut current_layer);
    }

    // 4. Construct butterfly encoding layer
    {
        let layer_index = num_layers as u32;

        butterfly_encode_layer(
            config,
            window_index,
            replica_id,
            layer_index,
            &previous_layer,
            data,
            &mut current_layer,
        )
        .context("failed to construct butterfly encoding layer")?;

        // drop previous, to reduce memory usage immediately
        drop(previous_layer);

        let store_config = StoreConfig::from_config(
            &store_config,
            CacheKey::label_layer_with_window(layer_index, window_index),
            tree_len,
        );
        let tree = lc_tree_from_slice(&current_layer, store_config)
            .context("failed to construct merkle tree for butterfly encoding layer")?;
        trees.push(tree);
    }

    Ok((current_layer, trees))
}

/// Decodes the provided `encoded_data`, returning the decoded data.
pub fn decode<H: Hasher>(
    config: &Config,
    window_index: u32,
    replica_id: &H::Domain,
    encoded_data: &[u8],
) -> Result<Vec<u8>> {
    let num_layers = config.num_layers();

    let mut previous_layer = vec![0u8; config.n];
    let mut current_layer = vec![0u8; config.n];

    // 1. Construct the mask
    mask_layer(config, window_index, replica_id, &mut previous_layer)
        .context("failed to construct mask")?;

    // 2. Construct expander layers
    for layer_index in 2..=(config.num_expander_layers as u32) {
        expander_layer(
            config,
            window_index,
            replica_id,
            layer_index,
            &previous_layer,
            &mut current_layer,
        )
        .context("failed to construct expander layer")?;

        // swap layers to reuse memory
        std::mem::swap(&mut previous_layer, &mut current_layer);
    }

    // 3. Construct butterfly layers
    for layer_index in (1 + config.num_expander_layers as u32)..(num_layers as u32) {
        butterfly_layer(
            config,
            window_index,
            replica_id,
            layer_index,
            &previous_layer,
            &mut current_layer,
        )
        .context("failed to construct butterfly layer")?;

        // swap layers to reuse memory
        std::mem::swap(&mut previous_layer, &mut current_layer);
    }

    // 4. Construct butterfly encoding layer
    {
        let layer_index = num_layers as u32;

        butterfly_decode_layer(
            config,
            window_index,
            replica_id,
            layer_index,
            &previous_layer,
            encoded_data,
            &mut current_layer,
        )
        .context("failed to construct butterfly decoding layer")?;
    }

    Ok(current_layer)
}

/// Generate the mask layer, for one window.
pub fn mask_layer<D: Domain>(
    config: &Config,
    window_index: u32,
    replica_id: &D,
    layer_out: &mut [u8],
) -> Result<()> {
    ensure!(
        layer_out.len() == config.n,
        "layer_out must be of size {}, got {}",
        config.n,
        layer_out.len()
    );

    // The mask layer is always layer 1.
    const LAYER_INDEX: u32 = 1;

    // Construct the mask
    for (node_index, node) in layer_out.chunks_mut(NODE_SIZE).enumerate() {
        let prefix = hash_prefix(LAYER_INDEX, node_index as u32, window_index);
        let hash = Sha256::digest(&[&prefix[..], AsRef::<[u8]>::as_ref(replica_id)]);
        node.copy_from_slice(&hash);
        truncate_hash(node);
    }

    Ok(())
}

/// Generate a single expander layer, for one window.
pub fn expander_layer<D: Domain>(
    config: &Config,
    window_index: u32,
    replica_id: &D,
    layer_index: u32,
    layer_in: &[u8],
    layer_out: &mut [u8],
) -> Result<()> {
    ensure!(
        layer_in.len() == layer_out.len(),
        "layer_in and layer_out must of the same size"
    );
    ensure!(
        layer_out.len() == config.n,
        "layer_out must be of size {}, got {}",
        config.n,
        layer_out.len()
    );
    ensure!(
        layer_index > 1 && layer_index as usize <= config.num_expander_layers,
        "layer index must be in range (1, {}], got {}",
        config.num_expander_layers,
        layer_index,
    );

    let graph: ExpanderGraph = config.into();

    // Iterate over each node.
    for (node_index, node) in layer_out.chunks_mut(NODE_SIZE).enumerate() {
        let node_index = node_index as u32;

        // Compute the parents for this node.
        let parents: Vec<_> = graph.parents(node_index).collect();

        let mut hasher = Sha256::new();

        // Hash prefix + replica id, each 32 bytes.
        let prefix = hash_prefix(layer_index, node_index, window_index);
        hasher.input(&[&prefix[..], AsRef::<[u8]>::as_ref(replica_id)]);

        // Compute batch hash of the parents.
        let hash = batch_hash(
            config.k as usize,
            config.degree_expander,
            hasher,
            &parents,
            layer_in,
        );
        node.copy_from_slice(&hash);
        truncate_hash(node);
    }

    Ok(())
}

/// Generate a single butterfly layer.
pub fn butterfly_layer<D: Domain>(
    config: &Config,
    window_index: u32,
    replica_id: &D,
    layer_index: u32,
    layer_in: &[u8],
    layer_out: &mut [u8],
) -> Result<()> {
    ensure!(
        layer_in.len() == layer_out.len(),
        "layer_in and layer_out must of the same size"
    );
    ensure!(
        layer_out.len() == config.n,
        "layer_out must be of size {}, got {}",
        config.n,
        layer_out.len()
    );
    ensure!(
        layer_index as usize > config.num_expander_layers
            && (layer_index as usize) < config.num_expander_layers + config.num_butterfly_layers,
        "layer index must be in range ({}, {}), got {}",
        config.num_expander_layers,
        config.num_expander_layers + config.num_butterfly_layers,
        layer_index,
    );

    let graph: ButterflyGraph = config.into();

    // Iterate over each node.
    for (node_index, node) in layer_out.chunks_mut(NODE_SIZE).enumerate() {
        let node_index = node_index as u32;

        let mut hasher = Sha256::new();

        // Hash prefix + replica id, each 32 bytes.
        let prefix = hash_prefix(layer_index, node_index, window_index);
        hasher.input(&[&prefix[..], AsRef::<[u8]>::as_ref(replica_id)]);

        // Compute hash of the parents.
        for (parent_a, parent_b) in graph.parents(node_index, layer_index).tuples() {
            dbg!(parent_a, parent_b, node_index, layer_index);
            let parent_a = parent_a as usize;
            let parent_b = parent_b as usize;
            let parent_a_value = &layer_in[parent_a * NODE_SIZE..(parent_a + 1) * NODE_SIZE];
            let parent_b_value = &layer_in[parent_b * NODE_SIZE..(parent_b + 1) * NODE_SIZE];

            hasher.input(&[parent_a_value, parent_b_value]);
        }

        let hash = hasher.finish();
        node.copy_from_slice(&hash);
        truncate_hash(node);
    }

    Ok(())
}

/// Generate a butterfly layer which additionally encodes using the data.
pub fn butterfly_encode_layer<D: Domain>(
    config: &Config,
    window_index: u32,
    replica_id: &D,
    layer_index: u32,
    layer_in: &[u8],
    data: &[u8],
    layer_out: &mut [u8],
) -> Result<()> {
    butterfly_encode_decode_layer(
        config,
        window_index,
        replica_id,
        layer_index,
        layer_in,
        data,
        layer_out,
        encode::encode,
    )
}

/// Generate a butterfly layer which additionally decodes using the data.
pub fn butterfly_decode_layer<D: Domain>(
    config: &Config,
    window_index: u32,
    replica_id: &D,
    layer_index: u32,
    layer_in: &[u8],
    data: &[u8],
    layer_out: &mut [u8],
) -> Result<()> {
    butterfly_encode_decode_layer(
        config,
        window_index,
        replica_id,
        layer_index,
        layer_in,
        data,
        layer_out,
        encode::decode,
    )
}

/// Generate a butterfly layer which additionally encodes or decodes using the data.
fn butterfly_encode_decode_layer<D: Domain, F: Fn(D, D) -> D>(
    config: &Config,
    window_index: u32,
    replica_id: &D,
    layer_index: u32,
    layer_in: &[u8],
    data: &[u8], // TODO: might want to overwrite the data
    layer_out: &mut [u8],
    op: F,
) -> Result<()> {
    ensure!(
        layer_in.len() == layer_out.len(),
        "layer_in and layer_out must of the same size"
    );
    ensure!(
        layer_out.len() == config.n,
        "layer_out must be of size {}, got {}",
        config.n,
        layer_out.len()
    );
    ensure!(
        layer_index as usize == config.num_expander_layers + config.num_butterfly_layers,
        "encoding must be on the last layer"
    );

    let graph: ButterflyGraph = config.into();

    // Iterate over each node.
    for (node_index, (node, data_node)) in layer_out
        .chunks_mut(NODE_SIZE)
        .zip(data.chunks(NODE_SIZE))
        .enumerate()
    {
        let node_index = node_index as u32;

        let mut hasher = Sha256::new();

        // Hash prefix + replica id, each 32 bytes.
        let prefix = hash_prefix(layer_index, node_index, window_index);
        hasher.input(&[&prefix[..], AsRef::<[u8]>::as_ref(replica_id)]);

        // Compute hash of the parents.
        for (parent_a, parent_b) in graph.parents(node_index, layer_index).tuples() {
            let parent_a = parent_a as usize;
            let parent_b = parent_b as usize;
            let parent_a_value = &layer_in[parent_a * NODE_SIZE..(parent_a + 1) * NODE_SIZE];
            let parent_b_value = &layer_in[parent_b * NODE_SIZE..(parent_b + 1) * NODE_SIZE];

            hasher.input(&[parent_a_value, parent_b_value]);
        }

        let mut key = hasher.finish();
        truncate_hash(&mut key);

        // encode
        let key = D::try_from_bytes(&key)?;
        let data_node = D::try_from_bytes(data_node)?;
        let encoded_node = op(key, data_node);

        // write result
        node.copy_from_slice(AsRef::<[u8]>::as_ref(&encoded_node));
    }

    Ok(())
}

/// Constructs the first 32 byte prefix for hashing any node.
pub fn hash_prefix(layer: u32, node_index: u32, window_index: u32) -> [u8; 32] {
    let mut prefix = [0u8; 32];
    // layer: 32bits
    prefix[..4].copy_from_slice(&layer.to_be_bytes());
    // node_index: 32bits
    prefix[4..8].copy_from_slice(&node_index.to_be_bytes());
    // window_index: 32bits
    prefix[8..12].copy_from_slice(&window_index.to_be_bytes());
    // 0 padding for the rest

    prefix
}

/// Construct an oct level cache tree from the given byte slice.
fn lc_tree_from_slice<H: 'static + Hasher>(
    data: &[u8],
    config: StoreConfig,
) -> Result<OctLCMerkleTree<H>> {
    OctLCMerkleTree::<H>::from_par_iter_with_config(
        data.par_chunks(NODE_SIZE)
            .map(|node| H::Domain::try_from_bytes(node).expect("invalid data")),
        config,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use paired::bls12_381::Fr;
    use rand::{Rng, SeedableRng};
    use rand_xorshift::XorShiftRng;
    use storage_proofs_core::{
        fr32::fr_into_bytes,
        hasher::{PoseidonDomain, PoseidonHasher, Sha256Domain},
    };

    fn sample_config() -> Config {
        Config {
            k: 8,
            n: 2048,
            degree_expander: 12,
            degree_butterfly: 4,
            num_expander_layers: 6,
            num_butterfly_layers: 4,
        }
    }

    #[test]
    fn test_mask_layer() {
        let rng = &mut XorShiftRng::from_seed(crate::TEST_SEED);

        let config = sample_config();
        let replica_id: Sha256Domain = Fr::random(rng).into();
        let window_index = rng.gen();

        let mut layer: Vec<u8> = (0..config.n).map(|_| rng.gen()).collect();

        mask_layer(&config, window_index, &replica_id, &mut layer).unwrap();

        assert!(!layer.iter().all(|&byte| byte == 0), "must not all be zero");
    }

    #[test]
    fn test_expander_layer() {
        let rng = &mut XorShiftRng::from_seed(crate::TEST_SEED);

        let config = sample_config();
        let replica_id: Sha256Domain = Fr::random(rng).into();
        let window_index = rng.gen();
        let layer_index = rng.gen_range(2, config.num_expander_layers as u32);

        let layer_in: Vec<u8> = (0..config.n / 32)
            .flat_map(|_| fr_into_bytes(&Fr::random(rng)))
            .collect();
        let mut layer_out = vec![0u8; config.n];

        expander_layer(
            &config,
            window_index,
            &replica_id,
            layer_index,
            &layer_in,
            &mut layer_out,
        )
        .unwrap();

        assert!(
            !layer_out.iter().all(|&byte| byte == 0),
            "must not all be zero"
        );
    }

    #[test]
    fn test_butterfly_layer() {
        let rng = &mut XorShiftRng::from_seed(crate::TEST_SEED);

        let config = sample_config();
        let replica_id: Sha256Domain = Fr::random(rng).into();
        let window_index = rng.gen();
        let layer_index = rng.gen_range(
            config.num_expander_layers,
            config.num_expander_layers + config.num_butterfly_layers,
        ) as u32;

        let layer_in: Vec<u8> = (0..config.n / 32)
            .flat_map(|_| fr_into_bytes(&Fr::random(rng)))
            .collect();
        let mut layer_out = vec![0u8; config.n];

        butterfly_layer(
            &config,
            window_index,
            &replica_id,
            layer_index,
            &layer_in,
            &mut layer_out,
        )
        .unwrap();

        assert!(
            !layer_out.iter().all(|&byte| byte == 0),
            "must not all be zero"
        );
    }

    #[test]
    fn test_butterfly_encode_decode_layer() {
        let rng = &mut XorShiftRng::from_seed(crate::TEST_SEED);

        let config = sample_config();
        let replica_id: Sha256Domain = Fr::random(rng).into();
        let window_index = rng.gen();
        let layer_index = (config.num_expander_layers + config.num_butterfly_layers) as u32;

        let data: Vec<u8> = (0..config.n / 32)
            .flat_map(|_| fr_into_bytes(&Fr::random(rng)))
            .collect();

        let layer_in: Vec<u8> = (0..config.n / 32)
            .flat_map(|_| fr_into_bytes(&Fr::random(rng)))
            .collect();

        let mut layer_out = vec![0u8; config.n];

        butterfly_encode_layer(
            &config,
            window_index,
            &replica_id,
            layer_index,
            &layer_in,
            &data,
            &mut layer_out,
        )
        .unwrap();

        assert!(
            !layer_out.iter().all(|&byte| byte == 0),
            "must not all be zero"
        );

        let mut data_back = vec![0u8; config.n];
        butterfly_decode_layer(
            &config,
            window_index,
            &replica_id,
            layer_index,
            &layer_in,
            &layer_out,
            &mut data_back,
        )
        .unwrap();
        assert_eq!(data, data_back, "failed to decode");
    }

    #[test]
    fn test_encode_decode_layer() {
        let rng = &mut XorShiftRng::from_seed(crate::TEST_SEED);

        let config = sample_config();
        let replica_id: PoseidonDomain = Fr::random(rng).into();
        let window_index = rng.gen();

        let data: Vec<u8> = (0..config.n / 32)
            .flat_map(|_| fr_into_bytes(&Fr::random(rng)))
            .collect();

        let cache_dir = tempfile::tempdir().unwrap();
        let store_config = StoreConfig::new(
            cache_dir.path(),
            CacheKey::CommDTree.to_string(),
            StoreConfig::default_cached_above_base_layer(config.n / NODE_SIZE, 8),
        );
        let (encoded_data, trees) = encode_with_trees::<PoseidonHasher>(
            &config,
            store_config,
            window_index,
            &replica_id,
            &data,
        )
        .unwrap();
        assert_eq!(
            trees.len(),
            config.num_expander_layers + config.num_butterfly_layers
        );
        assert_ne!(data, encoded_data, "failed to encode");

        let data_back =
            decode::<PoseidonHasher>(&config, window_index, &replica_id, &encoded_data).unwrap();
        assert_eq!(data, data_back, "failed to decode");
    }

    #[test]
    fn test_hash_prefix() {
        assert_eq!(hash_prefix(0, 0, 0), [0u8; 32]);
        assert_eq!(
            hash_prefix(1, 2, 3),
            [
                0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0
            ]
        );
    }
}