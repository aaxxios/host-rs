use crate::collection::Registration;
use crate::container::index_by_key;
use crate::key::KeyMap;
use crate::{Dependency, Lifetime};

/// The dependency graph of a collection, built once per `build` and shared by
/// every check.
pub(crate) struct Graph<'a> {
    registrations: &'a [Registration],
    /// Declared dependencies per registration; `None` for factories and
    /// instances, whose dependencies are unknown.
    dependencies: Vec<Option<Vec<Dependency>>>,
    by_key: KeyMap<Box<[usize]>>,
    /// A single dependency points at the last registration of its service, a
    /// dependency on all of them at each one.
    edges: Vec<Vec<usize>>,
}

impl<'a> Graph<'a> {
    pub(crate) fn new(registrations: &'a [Registration]) -> Self {
        let by_key = index_by_key(registrations);
        let dependencies: Vec<_> = registrations
            .iter()
            .map(|registration| registration.dependencies.map(|dependencies| dependencies()))
            .collect();
        let edges = dependencies
            .iter()
            .map(|dependencies| {
                let mut targets = Vec::new();
                for dependency in dependencies.iter().flatten() {
                    let Some(indices) = by_key.get(&dependency.key()) else {
                        continue;
                    };
                    if dependency.is_all() {
                        targets.extend_from_slice(indices);
                    } else {
                        targets.extend(indices.last());
                    }
                }
                targets
            })
            .collect();
        Self {
            registrations,
            dependencies,
            by_key,
            edges,
        }
    }

    /// Dependency cycles, each reported once, rotated to start at its earliest
    /// registration and closed by repeating that service. Cycles through
    /// factories and instances are left to resolution time.
    pub(crate) fn cycles(&self) -> Vec<Vec<&'static str>> {
        let mut state = vec![Visit::New; self.registrations.len()];
        let mut cycles: Vec<Vec<usize>> = Vec::new();

        for root in 0..self.registrations.len() {
            if state[root] != Visit::New {
                continue;
            }
            let mut stack = vec![(root, 0)];
            state[root] = Visit::OnStack;
            while let Some(&mut (node, ref mut next)) = stack.last_mut() {
                let Some(&target) = self.edges[node].get(*next) else {
                    state[node] = Visit::Done;
                    stack.pop();
                    continue;
                };
                *next += 1;
                match state[target] {
                    Visit::New => {
                        state[target] = Visit::OnStack;
                        stack.push((target, 0));
                    }
                    Visit::OnStack => {
                        let start = stack.iter().position(|&(n, _)| n == target).unwrap_or(0);
                        let cycle = normalize(stack[start..].iter().map(|&(n, _)| n).collect());
                        if !cycles.contains(&cycle) {
                            cycles.push(cycle);
                        }
                    }
                    Visit::Done => {}
                }
            }
        }

        cycles.sort();
        cycles
            .into_iter()
            .map(|cycle| {
                let mut path: Vec<_> = cycle.iter().map(|&n| self.name(n)).collect();
                path.push(self.name(cycle[0]));
                path
            })
            .collect()
    }

    /// Declared single dependencies that have no registration, as
    /// `[service, dependency]`, in registration order. Identical findings,
    /// from a repeated dependency or registration, are reported once.
    pub(crate) fn missing(&self) -> Vec<Vec<&'static str>> {
        let mut missing = Vec::new();
        for (index, dependencies) in self.dependencies.iter().enumerate() {
            for dependency in dependencies.iter().flatten() {
                let path = vec![self.name(index), dependency.type_name()];
                if !dependency.is_all()
                    && !self.by_key.contains_key(&dependency.key())
                    && !missing.contains(&path)
                {
                    missing.push(path);
                }
            }
        }
        missing
    }

    /// Singletons that reach a scoped service directly or through transients,
    /// as `[singleton, transient…, scoped]`, in registration order. A singleton
    /// reached along the way is not followed: it is checked as its own root.
    pub(crate) fn captive(&self) -> Vec<Vec<&'static str>> {
        let mut captive = Vec::new();
        for (root, registration) in self.registrations.iter().enumerate() {
            if registration.lifetime != Lifetime::Singleton || self.dependencies[root].is_none() {
                continue;
            }
            let mut visited = vec![false; self.registrations.len()];
            visited[root] = true;
            let mut stack = vec![(root, 0)];
            while let Some(&mut (node, ref mut next)) = stack.last_mut() {
                let Some(&target) = self.edges[node].get(*next) else {
                    stack.pop();
                    continue;
                };
                *next += 1;
                if visited[target] {
                    continue;
                }
                visited[target] = true;
                match self.registrations[target].lifetime {
                    Lifetime::Scoped => {
                        let mut path: Vec<_> = stack.iter().map(|&(n, _)| self.name(n)).collect();
                        path.push(self.name(target));
                        if !captive.contains(&path) {
                            captive.push(path);
                        }
                    }
                    Lifetime::Transient => stack.push((target, 0)),
                    Lifetime::Singleton => {}
                }
            }
        }
        captive
    }

    fn name(&self, index: usize) -> &'static str {
        self.registrations[index].implementation
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    New,
    OnStack,
    Done,
}

fn normalize(mut cycle: Vec<usize>) -> Vec<usize> {
    if let Some(start) = cycle
        .iter()
        .enumerate()
        .min_by_key(|&(_, n)| n)
        .map(|(i, _)| i)
    {
        cycle.rotate_left(start);
    }
    cycle
}
