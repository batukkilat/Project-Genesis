//! ECS components of a particle — the only primitive simulation object.
//!
//! Per the constitution: no biological fields, no biological names. The
//! `memory` and `bonds` fields suggested by the master prompt are deferred to
//! Phase 3 (interaction system), where their semantics get defined.

use bevy_ecs::prelude::*;
use genesis_core::{ParticleId, Vec2};

/// Stable particle identity (see [`genesis_core::ParticleId`]).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub ParticleId);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Position(pub Vec2);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Velocity(pub Vec2);

/// Fundamental quantity: matter.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Matter(pub f32);

/// Fundamental quantity: energy.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Energy(pub f32);

/// Fundamental quantity: information. First-class, like mass in physics.
/// Carried as a scalar in Phase 1; semantics defined in Phase 3.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Information(pub f32);
