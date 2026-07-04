//! Layer 0 primitives for Project Genesis.
//!
//! Everything that defines a simulation's replay identity lives here:
//! the particle scalar types, the deterministic RNG, and the state hash.
//! This crate has no dependencies by design — see Cargo.toml.

pub mod hash;
pub mod rng;
pub mod vec2;

pub use hash::StateHasher;
pub use rng::DetRng;
pub use vec2::Vec2;

/// Unique, stable identifier of a particle within one simulation run.
///
/// Ids are assigned sequentially at spawn time and never reused, so sorting
/// by id gives a canonical particle order for hashing and serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParticleId(pub u64);
