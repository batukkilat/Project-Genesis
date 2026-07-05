//! Bond storage: persistent pairwise links with per-bond scalar state.
//!
//! Design (docs/research/bond-storage.md): a canonical SoA edge list is the
//! source of truth — endpoints are stable particle **ids** with `a < b`,
//! rows always sorted by `(a, b)`, so the layout is a pure function of state
//! and saves/replays stay bit-identical. A CSR adjacency mirror keyed by
//! post-canonical particle **index** is rebuilt every tick right after
//! `ParticleStore::canonicalize`; it is derived data, never saved, and is
//! what the force pass iterates (particle-major, disjoint writes — the same
//! shape as `cell_start`, so thread count cannot affect results).
//!
//! The id → index map is rebuilt each tick and is lookup-only: it is never
//! iterated, so hash order cannot leak into outcomes. An edge whose endpoint
//! no longer maps refers to a destroyed particle and is dropped during the
//! rebuild (lazy prune) — the edge list is self-healing by construction.

use bevy_ecs::prelude::*;
use rustc_hash::FxHashMap;

use crate::store::ParticleStore;

#[derive(Resource, Debug, Default)]
pub struct BondStore {
    // --- Saved state: SoA edge list, ids, a < b, sorted by (a, b).
    pub a: Vec<u64>,
    pub b: Vec<u64>,
    /// Spring stiffness (see `PhysicsParams::bond_rest_length`).
    pub strength: Vec<f32>,

    // --- Derived, rebuilt each tick after canonicalize; NOT saved.
    csr_start: Vec<u32>,
    csr_partner: Vec<u32>,
    csr_bond: Vec<u32>,
    idx_of_id: FxHashMap<u64, u32>,
    cursor: Vec<u32>,
}

impl BondStore {
    /// Number of bonds (edge-list rows).
    pub fn len(&self) -> usize {
        self.a.len()
    }

    pub fn is_empty(&self) -> bool {
        self.a.is_empty()
    }

    /// Append an edge without keeping the list sorted; call
    /// `sort_canonical` after bulk loading. Endpoint order is normalized.
    pub fn push(&mut self, a: u64, b: u64, strength: f32) {
        assert_ne!(a, b, "a particle cannot bond to itself");
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        self.a.push(lo);
        self.b.push(hi);
        self.strength.push(strength);
    }

    /// Restore the canonical (a, b) sort and drop duplicate pairs (first
    /// occurrence wins). Idempotent.
    pub fn sort_canonical(&mut self) {
        let n = self.len();
        let mut order: Vec<u32> = (0..n as u32).collect();
        order.sort_by_key(|&r| (self.a[r as usize], self.b[r as usize]));
        let a: Vec<u64> = order.iter().map(|&r| self.a[r as usize]).collect();
        let b: Vec<u64> = order.iter().map(|&r| self.b[r as usize]).collect();
        let s: Vec<f32> = order.iter().map(|&r| self.strength[r as usize]).collect();
        self.a = a;
        self.b = b;
        self.strength = s;
        let mut w = 0;
        for r in 0..n {
            if r > 0 && self.a[r] == self.a[w - 1] && self.b[r] == self.b[w - 1] {
                continue;
            }
            self.a[w] = self.a[r];
            self.b[w] = self.b[r];
            self.strength[w] = self.strength[r];
            w += 1;
        }
        self.a.truncate(w);
        self.b.truncate(w);
        self.strength.truncate(w);
    }

    fn find(&self, lo: u64, hi: u64) -> Result<usize, usize> {
        let mut left = 0usize;
        let mut right = self.len();
        while left < right {
            let mid = left + (right - left) / 2;
            match (self.a[mid], self.b[mid]).cmp(&(lo, hi)) {
                std::cmp::Ordering::Less => left = mid + 1,
                std::cmp::Ordering::Greater => right = mid,
                std::cmp::Ordering::Equal => return Ok(mid),
            }
        }
        Err(left)
    }

    pub fn contains(&self, a: u64, b: u64) -> bool {
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        self.find(lo, hi).is_ok()
    }

    /// Insert a bond, keeping the canonical sort. No-op if the pair is
    /// already bonded (bonds never stack). Returns whether it was inserted.
    /// O(len) per call from the memmove — fine while events per tick are
    /// sparse; batch if profiling ever says otherwise.
    pub fn create(&mut self, a: u64, b: u64, strength: f32) -> bool {
        assert_ne!(a, b, "a particle cannot bond to itself");
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        match self.find(lo, hi) {
            Ok(_) => false,
            Err(pos) => {
                self.a.insert(pos, lo);
                self.b.insert(pos, hi);
                self.strength.insert(pos, strength);
                true
            }
        }
    }

    /// Remove the pair's bond if present. Returns whether one was removed.
    pub fn remove(&mut self, a: u64, b: u64) -> bool {
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        match self.find(lo, hi) {
            Ok(pos) => {
                self.a.remove(pos);
                self.b.remove(pos);
                self.strength.remove(pos);
                true
            }
            Err(_) => false,
        }
    }

    /// Rebuild the derived structures against the store's current canonical
    /// layout: the id → index map, then the CSR mirror. Edges referring to
    /// ids no longer in the store are pruned. Must run every tick after
    /// `ParticleStore::canonicalize`, before anything reads the CSR.
    pub fn rebuild(&mut self, store: &ParticleStore) {
        let n = store.len();
        self.idx_of_id.clear();
        self.idx_of_id.reserve(n);
        for (i, &id) in store.id.iter().enumerate() {
            self.idx_of_id.insert(id, i as u32);
        }

        // Lazy prune of dangling edges (destroyed endpoints). Order-stable
        // compaction, so the canonical sort survives.
        let mut w = 0;
        for r in 0..self.a.len() {
            if self.idx_of_id.contains_key(&self.a[r]) && self.idx_of_id.contains_key(&self.b[r]) {
                self.a[w] = self.a[r];
                self.b[w] = self.b[r];
                self.strength[w] = self.strength[r];
                w += 1;
            }
        }
        self.a.truncate(w);
        self.b.truncate(w);
        self.strength.truncate(w);

        // CSR, both directions, via the same counting-sort idiom as
        // `cell_start`: count degrees, prefix-sum, fill with a cursor.
        self.csr_start.clear();
        self.csr_start.resize(n + 1, 0);
        for r in 0..w {
            let ia = self.idx_of_id[&self.a[r]] as usize;
            let ib = self.idx_of_id[&self.b[r]] as usize;
            self.csr_start[ia + 1] += 1;
            self.csr_start[ib + 1] += 1;
        }
        for i in 1..=n {
            self.csr_start[i] += self.csr_start[i - 1];
        }
        self.cursor.clear();
        self.cursor.extend_from_slice(&self.csr_start[..n]);
        self.csr_partner.clear();
        self.csr_partner.resize(2 * w, 0);
        self.csr_bond.clear();
        self.csr_bond.resize(2 * w, 0);
        for r in 0..w {
            let ia = self.idx_of_id[&self.a[r]] as usize;
            let ib = self.idx_of_id[&self.b[r]] as usize;
            let ca = self.cursor[ia] as usize;
            self.csr_partner[ca] = ib as u32;
            self.csr_bond[ca] = r as u32;
            self.cursor[ia] += 1;
            let cb = self.cursor[ib] as usize;
            self.csr_partner[cb] = ia as u32;
            self.csr_bond[cb] = r as u32;
            self.cursor[ib] += 1;
        }
    }

    /// CSR row for particle index `i`: (partner index, edge-list row) pairs
    /// in canonical order. Valid until the next edge-list mutation or
    /// `rebuild`.
    pub fn partners_of(&self, i: usize) -> impl Iterator<Item = (u32, u32)> + '_ {
        let lo = self.csr_start[i] as usize;
        let hi = self.csr_start[i + 1] as usize;
        self.csr_partner[lo..hi]
            .iter()
            .copied()
            .zip(self.csr_bond[lo..hi].iter().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridGeom;

    fn store_with_ids(ids: &[u64]) -> ParticleStore {
        let geom = GridGeom::new(30.0, 30.0, 10.0);
        let mut s = ParticleStore::default();
        for (k, &id) in ids.iter().enumerate() {
            s.push(id, k as f32 * 2.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        }
        s.canonicalize(&geom);
        s
    }

    #[test]
    fn create_normalizes_sorts_and_dedups() {
        let mut bonds = BondStore::default();
        assert!(bonds.create(5, 2, 1.0)); // reversed endpoints
        assert!(bonds.create(1, 2, 2.0));
        assert!(!bonds.create(2, 5, 9.0)); // duplicate of (2, 5), no stack
        assert_eq!(bonds.a, vec![1, 2]);
        assert_eq!(bonds.b, vec![2, 5]);
        assert_eq!(bonds.strength, vec![2.0, 1.0]);
        assert!(bonds.contains(5, 2));
        assert!(!bonds.contains(1, 5));
    }

    #[test]
    fn remove_deletes_the_pair() {
        let mut bonds = BondStore::default();
        bonds.create(1, 2, 1.0);
        bonds.create(2, 3, 1.0);
        assert!(bonds.remove(2, 1));
        assert!(!bonds.remove(2, 1));
        assert_eq!(bonds.len(), 1);
        assert!(bonds.contains(2, 3));
    }

    #[test]
    fn rebuild_builds_csr_both_directions() {
        let s = store_with_ids(&[10, 20, 30]);
        let mut bonds = BondStore::default();
        bonds.create(10, 20, 1.5);
        bonds.create(20, 30, 2.5);
        bonds.rebuild(&s);

        let idx = |id: u64| s.id.iter().position(|&x| x == id).unwrap();
        let of = |id: u64| bonds.partners_of(idx(id)).collect::<Vec<_>>();
        assert_eq!(of(10).len(), 1);
        assert_eq!(of(20).len(), 2);
        assert_eq!(of(30).len(), 1);
        let (p, row) = of(10)[0];
        assert_eq!(p as usize, idx(20));
        assert_eq!(bonds.strength[row as usize], 1.5);
        let (p, row) = of(30)[0];
        assert_eq!(p as usize, idx(20));
        assert_eq!(bonds.strength[row as usize], 2.5);
    }

    #[test]
    fn rebuild_prunes_dangling_edges() {
        let s = store_with_ids(&[10, 30]); // 20 does not exist
        let mut bonds = BondStore::default();
        bonds.create(10, 20, 1.0);
        bonds.create(10, 30, 2.0);
        bonds.rebuild(&s);
        assert_eq!(bonds.len(), 1);
        assert!(bonds.contains(10, 30));
        assert_eq!(bonds.partners_of(0).count(), 1);
    }

    #[test]
    fn sort_canonical_orders_and_dedups_bulk_pushes() {
        let mut bonds = BondStore::default();
        bonds.push(7, 3, 1.0);
        bonds.push(1, 2, 2.0);
        bonds.push(3, 7, 9.0); // duplicate, first wins
        bonds.sort_canonical();
        assert_eq!(bonds.a, vec![1, 3]);
        assert_eq!(bonds.b, vec![2, 7]);
        assert_eq!(bonds.strength, vec![2.0, 1.0]);
    }
}
