use crate::models::Service;
use std::collections::{HashMap, HashSet};

/// Validates a proposed dependency set before it is persisted.
pub fn validate_graph(
    service_id: &str,
    depends_on: &[String],
    known_ids: &HashSet<String>,
    edges: &[(String, String)],
) -> Result<(), String> {
    for dependency in depends_on {
        if dependency == service_id {
            return Err("A service cannot depend on itself".into());
        }
        if !known_ids.contains(dependency) {
            return Err("A selected dependency no longer exists".into());
        }
    }
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for (from, to) in edges {
        if from != service_id {
            graph.entry(from.clone()).or_default().push(to.clone());
        }
    }
    graph.insert(service_id.to_string(), depends_on.to_vec());
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for node in graph.keys() {
        detect_cycle(node, &graph, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn detect_cycle(
    node: &str,
    graph: &HashMap<String, Vec<String>>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) -> Result<(), String> {
    if visited.contains(node) {
        return Ok(());
    }
    if !visiting.insert(node.to_string()) {
        return Err("Dependency cycle detected".into());
    }
    if let Some(dependencies) = graph.get(node) {
        for dependency in dependencies {
            detect_cycle(dependency, graph, visiting, visited)?;
        }
    }
    visiting.remove(node);
    visited.insert(node.to_string());
    Ok(())
}

/// Detects cycles in a complete proposed edge set.
pub fn validate_edges(edges: &[(String, String)]) -> Result<(), String> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for (from, to) in edges {
        graph.entry(from.clone()).or_default().push(to.clone());
    }
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for node in graph.keys() {
        detect_cycle(node, &graph, &mut visiting, &mut visited)?;
    }
    Ok(())
}

/// Resolves the targets and all transitive dependencies into start order.
/// Dependencies come before the services that need them.
pub fn start_order(
    targets: &[String],
    services: &[Service],
    edges: &[(String, String)],
) -> Result<Vec<String>, String> {
    let by_id: HashMap<&str, &Service> = services
        .iter()
        .map(|service| (service.id.as_str(), service))
        .collect();
    let order_index: HashMap<&str, usize> = services
        .iter()
        .enumerate()
        .map(|(index, service)| (service.id.as_str(), index))
        .collect();
    let mut dependencies: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, to) in edges {
        dependencies.entry(from).or_default().push(to);
    }
    for list in dependencies.values_mut() {
        list.sort_by_key(|dependency| order_index.get(dependency).copied().unwrap_or(usize::MAX));
    }
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut order = Vec::new();
    for target in targets {
        visit(
            target,
            &by_id,
            &dependencies,
            &mut visiting,
            &mut visited,
            &mut order,
        )?;
    }
    Ok(order.iter().map(|id| (*id).to_string()).collect())
}

fn visit<'a>(
    id: &str,
    by_id: &HashMap<&'a str, &'a Service>,
    dependencies: &HashMap<&'a str, Vec<&'a str>>,
    visiting: &mut HashSet<&'a str>,
    visited: &mut HashSet<&'a str>,
    order: &mut Vec<&'a str>,
) -> Result<(), String> {
    if visited.contains(id) {
        return Ok(());
    }
    let Some(service) = by_id.get(id) else {
        return Err("A required service no longer exists".into());
    };
    if !service.enabled {
        return Err(format!(
            "{} is disabled and cannot satisfy a dependency",
            service.name
        ));
    }
    if !visiting.insert(service.id.as_str()) {
        return Err(format!("Dependency cycle detected at {}", service.name));
    }
    if let Some(list) = dependencies.get(id) {
        for dependency in list {
            visit(dependency, by_id, dependencies, visiting, visited, order)?;
        }
    }
    visiting.remove(service.id.as_str());
    visited.insert(service.id.as_str());
    order.push(service.id.as_str());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateService, Service};

    fn service(name: &str) -> Service {
        Service::from_create(CreateService {
            name: name.into(),
            icon_path: None,
            executable: "/bin/echo".into(),
            arguments: vec![],
            working_directory: None,
            environment: vec![],
            description: None,
            enabled: true,
            auto_start: false,
            restart_policy: "never".into(),
            restart_delay_ms: 1000,
            max_restart_attempts: Some(5),
            priority: "normal".into(),
            stop_grace_period_ms: 3000,
            log_max_size_mb: 10,
            health_check_type: "none".into(),
            health_target: None,
            health_interval_ms: 5000,
            health_timeout_ms: 2000,
            health_expected_status: None,
            health_start_period_ms: 5000,
            depends_on: vec![],
        })
        .expect("valid service")
    }

    fn edge(from: &Service, to: &Service) -> (String, String) {
        (from.id.clone(), to.id.clone())
    }

    #[test]
    fn orders_dependencies_before_dependents() {
        let database = service("Database");
        let api = service("API");
        let web = service("Web");
        let edges = vec![edge(&api, &database), edge(&web, &api)];
        let order = start_order(
            std::slice::from_ref(&web.id),
            &[database.clone(), api.clone(), web.clone()],
            &edges,
        )
        .expect("resolve order");
        assert_eq!(order, vec![database.id, api.id, web.id]);
    }

    #[test]
    fn rejects_cycles_and_missing_services() {
        let a = service("A");
        let b = service("B");
        let edges = vec![edge(&a, &b), edge(&b, &a)];
        assert!(
            start_order(std::slice::from_ref(&a.id), &[a.clone(), b.clone()], &edges)
                .unwrap_err()
                .contains("cycle")
        );

        assert!(
            start_order(&["missing".into()], std::slice::from_ref(&a), &[])
                .unwrap_err()
                .contains("no longer exists")
        );
    }

    #[test]
    fn rejects_disabled_dependencies() {
        let database = service("Database");
        let mut api = service("API");
        api.enabled = false;
        let edges = vec![edge(&api, &database)];
        assert!(
            start_order(&[api.id.clone()], &[database, api], &edges)
                .unwrap_err()
                .contains("disabled")
        );
    }

    #[test]
    fn validates_existence_and_cycles_before_saving() {
        let a = service("A");
        let b = service("B");
        let known = HashSet::from([a.id.clone(), b.id.clone()]);
        assert!(validate_graph(&a.id, std::slice::from_ref(&b.id), &known, &[]).is_ok());
        assert!(
            validate_graph(&a.id, &["missing".into()], &known, &[])
                .unwrap_err()
                .contains("no longer exists")
        );
        let edges = vec![edge(&b, &a)];
        assert!(
            validate_graph(&a.id, std::slice::from_ref(&b.id), &known, &edges)
                .unwrap_err()
                .contains("cycle")
        );
    }
}
