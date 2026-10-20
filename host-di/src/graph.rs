use crate::Lifetime;
use crate::collection::Registration;
use crate::container::index_by_key;

/// Finds dependency cycles among registrations.
///
/// Each cycle is reported once, rotated to start at its earliest registration
/// and closed by repeating that service. Factories and instances declare no
/// dependencies, so cycles through them are left to resolution time.
pub(crate) fn find_cycles(registrations: &[Registration]) -> Vec<Vec<&'static str>> {
    let edges = edges(registrations);

    let mut state = vec![Visit::New; registrations.len()];
    let mut cycles: Vec<Vec<usize>> = Vec::new();

    for root in 0..registrations.len() {
        if state[root] != Visit::New {
            continue;
        }
        let mut stack = vec![(root, 0)];
        state[root] = Visit::OnStack;
        while let Some(&mut (node, ref mut next)) = stack.last_mut() {
            let Some(&target) = edges[node].get(*next) else {
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
            let mut path: Vec<_> = cycle
                .iter()
                .map(|&n| registrations[n].implementation)
                .collect();
            path.push(registrations[cycle[0]].implementation);
            path
        })
        .collect()
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

/// Declared single dependencies that have no registration, as
/// `[service, dependency]`, in registration order.
pub(crate) fn find_missing(registrations: &[Registration]) -> Vec<Vec<&'static str>> {
    let by_key = index_by_key(registrations);
    let mut missing = Vec::new();
    for registration in registrations {
        let Some(dependencies) = registration.dependencies else {
            continue;
        };
        for dependency in dependencies() {
            if !dependency.is_all() && !by_key.contains_key(&dependency.key()) {
                missing.push(vec![registration.implementation, dependency.type_name()]);
            }
        }
    }
    missing
}

/// Singletons that reach a scoped service directly or through transients, as
/// `[singleton, transient…, scoped]`, in registration order. A singleton
/// reached along the way is not followed: it is checked as its own root.
pub(crate) fn find_captive(registrations: &[Registration]) -> Vec<Vec<&'static str>> {
    let edges = edges(registrations);
    let mut captive = Vec::new();
    for (root, registration) in registrations.iter().enumerate() {
        if registration.lifetime != Lifetime::Singleton || registration.dependencies.is_none() {
            continue;
        }
        let mut visited = vec![false; registrations.len()];
        visited[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some(&mut (node, ref mut next)) = stack.last_mut() {
            let Some(&target) = edges[node].get(*next) else {
                stack.pop();
                continue;
            };
            *next += 1;
            if visited[target] {
                continue;
            }
            visited[target] = true;
            let dependency = &registrations[target];
            match dependency.lifetime {
                Lifetime::Scoped => {
                    let mut path: Vec<_> = stack
                        .iter()
                        .map(|&(n, _)| registrations[n].implementation)
                        .collect();
                    path.push(dependency.implementation);
                    captive.push(path);
                }
                Lifetime::Transient => stack.push((target, 0)),
                Lifetime::Singleton => {}
            }
        }
    }
    captive
}

/// Dependency edges between registrations: a single dependency points at the
/// last registration of its service, a dependency on all of them at each one.
/// Factories and instances declare no dependencies, so they have no edges.
fn edges(registrations: &[Registration]) -> Vec<Vec<usize>> {
    let by_key = index_by_key(registrations);
    registrations
        .iter()
        .map(|registration| {
            let dependencies = registration
                .dependencies
                .map(|dependencies| dependencies())
                .unwrap_or_default();
            let mut targets = Vec::new();
            for dependency in dependencies {
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
        .collect()
}
