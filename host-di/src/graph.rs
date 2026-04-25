use crate::collection::Registration;
use crate::key::KeyMap;

/// Finds dependency cycles among the effective (last-wins) registrations.
///
/// Each cycle is reported once, rotated to start at its earliest registration
/// and closed by repeating that service. Factories and instances declare no
/// dependencies, so cycles through them are left to resolution time.
pub(crate) fn find_cycles(registrations: &[Registration]) -> Vec<Vec<&'static str>> {
    let mut effective = KeyMap::default();
    for (index, registration) in registrations.iter().enumerate() {
        effective.insert(registration.key, index);
    }

    let edges: Vec<Vec<usize>> = registrations
        .iter()
        .map(|registration| {
            registration
                .dependencies
                .map(|dependencies| dependencies())
                .unwrap_or_default()
                .iter()
                .filter_map(|dependency| effective.get(&dependency.key()).copied())
                .collect()
        })
        .collect();

    let mut state = vec![Visit::New; registrations.len()];
    let mut cycles: Vec<Vec<usize>> = Vec::new();
    let mut roots: Vec<usize> = effective.values().copied().collect();
    roots.sort_unstable();

    for root in roots {
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
            let mut path: Vec<_> = cycle.iter().map(|&n| registrations[n].key.name).collect();
            path.push(registrations[cycle[0]].key.name);
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
