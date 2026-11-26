use crate::models::{ActiveProcessRecord, EnvironmentVariable, RunHistory, Service};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use std::path::Path;

const SCHEMA_VERSION: i64 = 6;

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let mut database = Self { connection };
        database.migrate()?;
        Ok(database)
    }

    fn migrate(&mut self) -> rusqlite::Result<()> {
        let version: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(rusqlite::Error::InvalidQuery);
        }
        if version == 0 {
            let tx = self.connection.transaction()?;
            tx.execute_batch(
                "CREATE TABLE service_groups (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT, icon_path TEXT,
                    sort_order INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                );
                CREATE TABLE services (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT, icon_path TEXT,
                    group_id TEXT REFERENCES service_groups(id) ON DELETE SET NULL,
                    execution_mode TEXT NOT NULL DEFAULT 'direct', executable TEXT NOT NULL,
                    arguments_json TEXT NOT NULL DEFAULT '[]', shell_command TEXT, working_directory TEXT,
                    enabled INTEGER NOT NULL DEFAULT 1, auto_start INTEGER NOT NULL DEFAULT 0,
                    restart_policy TEXT NOT NULL DEFAULT 'never', restart_delay_ms INTEGER NOT NULL DEFAULT 1000,
                    max_restart_attempts INTEGER, priority TEXT NOT NULL DEFAULT 'normal',
                    stop_grace_period_ms INTEGER NOT NULL DEFAULT 3000,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                    sort_order INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE service_environment (
                    id TEXT PRIMARY KEY, service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
                    key TEXT NOT NULL, value TEXT NOT NULL, is_secret INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                    UNIQUE(service_id, key)
                );
                CREATE TABLE service_dependencies (
                    service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
                    depends_on_service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
                    dependency_type TEXT NOT NULL DEFAULT 'start_before',
                    PRIMARY KEY(service_id, depends_on_service_id), CHECK(service_id != depends_on_service_id)
                );
                CREATE TABLE app_settings (
                    key TEXT PRIMARY KEY, value_json TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                );
                CREATE INDEX services_group_order ON services(group_id, sort_order);
                CREATE UNIQUE INDEX services_name_unique ON services(name COLLATE NOCASE);
                PRAGMA user_version = 1;"
            )?;
            tx.commit()?;
        }
        if version < 2 {
            let tx = self.connection.transaction()?;
            tx.execute_batch(
                "CREATE TABLE service_run_history (
                    id TEXT PRIMARY KEY,
                    service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
                    started_at_ms INTEGER NOT NULL,
                    ended_at_ms INTEGER NOT NULL,
                    status TEXT NOT NULL,
                    exit_code INTEGER,
                    error TEXT
                );
                CREATE INDEX service_run_history_recent ON service_run_history(service_id, started_at_ms DESC);
                PRAGMA user_version = 2;",
            )?;
            tx.commit()?;
        }
        if version < 3 {
            let tx = self.connection.transaction()?;
            tx.execute_batch(
                "CREATE TABLE service_active_processes (
                    service_id TEXT PRIMARY KEY REFERENCES services(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL,
                    process_group_id INTEGER NOT NULL,
                    process_start_time_s INTEGER NOT NULL,
                    executable_path TEXT NOT NULL
                );
                PRAGMA user_version = 3;",
            )?;
            tx.commit()?;
        }
        if version < 4 {
            let tx = self.connection.transaction()?;
            tx.execute_batch(
                "ALTER TABLE service_active_processes ADD COLUMN manager_session_id TEXT NOT NULL DEFAULT '';
                PRAGMA user_version = 4;",
            )?;
            tx.commit()?;
        }
        if version < 5 {
            // Version 3/4 stored sysinfo's user group id instead of the process
            // group id. Those records cannot be verified safely.
            let tx = self.connection.transaction()?;
            tx.execute_batch(
                "DELETE FROM service_active_processes;
                PRAGMA user_version = 5;",
            )?;
            tx.commit()?;
        }
        if version < 6 {
            let tx = self.connection.transaction()?;
            tx.execute_batch(
                "ALTER TABLE services ADD COLUMN log_max_size_mb INTEGER NOT NULL DEFAULT 10;
                PRAGMA user_version = 6;",
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn save_active_process(&mut self, record: &ActiveProcessRecord) -> rusqlite::Result<()> {
        let process_start_time_s = i64::try_from(record.process_start_time_s)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        self.connection.execute(
            "INSERT INTO service_active_processes (service_id,pid,process_group_id,process_start_time_s,executable_path,manager_session_id)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(service_id) DO UPDATE SET pid=excluded.pid,process_group_id=excluded.process_group_id,
               process_start_time_s=excluded.process_start_time_s,executable_path=excluded.executable_path,
               manager_session_id=excluded.manager_session_id",
            params![record.service_id, i64::from(record.pid), i64::from(record.process_group_id), process_start_time_s, record.executable_path, record.manager_session_id],
        )?;
        Ok(())
    }

    pub fn get_active_process(
        &self,
        service_id: &str,
    ) -> rusqlite::Result<Option<ActiveProcessRecord>> {
        self.connection
            .query_row(
                "SELECT service_id,pid,process_group_id,process_start_time_s,executable_path,manager_session_id
                 FROM service_active_processes WHERE service_id=?1",
                [service_id],
                active_process_from_row,
            )
            .optional()
    }

    pub fn list_active_processes(&self) -> rusqlite::Result<Vec<ActiveProcessRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT service_id,pid,process_group_id,process_start_time_s,executable_path,manager_session_id
             FROM service_active_processes ORDER BY service_id",
        )?;
        statement.query_map([], active_process_from_row)?.collect()
    }

    pub fn delete_active_process(&mut self, service_id: &str) -> rusqlite::Result<bool> {
        Ok(self.connection.execute(
            "DELETE FROM service_active_processes WHERE service_id=?1",
            [service_id],
        )? > 0)
    }

    pub fn insert_run_history(&mut self, run: &RunHistory) -> rusqlite::Result<()> {
        let started_at_ms = millis_to_sql(
            u64::try_from(run.started_at_ms)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?,
        )?;
        let ended_at_ms = millis_to_sql(
            u64::try_from(run.ended_at_ms)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?,
        )?;
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO service_run_history (id,service_id,started_at_ms,ended_at_ms,status,exit_code,error)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![run.id, run.service_id, started_at_ms, ended_at_ms, run.status, run.exit_code, run.error],
        )?;
        tx.execute(
            "DELETE FROM service_run_history WHERE service_id=?1 AND id NOT IN
             (SELECT id FROM service_run_history WHERE service_id=?1 ORDER BY started_at_ms DESC LIMIT 20)",
            [&run.service_id],
        )?;
        tx.commit()
    }

    pub fn list_run_history(&self, service_id: &str) -> rusqlite::Result<Vec<RunHistory>> {
        let mut statement = self.connection.prepare(
            "SELECT id,service_id,started_at_ms,ended_at_ms,status,exit_code,error
             FROM service_run_history WHERE service_id=?1 ORDER BY started_at_ms DESC LIMIT 20",
        )?;
        statement
            .query_map([service_id], |row| {
                Ok(RunHistory {
                    id: row.get(0)?,
                    service_id: row.get(1)?,
                    started_at_ms: row.get::<_, i64>(2)? as u128,
                    ended_at_ms: row.get::<_, i64>(3)? as u128,
                    status: row.get(4)?,
                    exit_code: row.get(5)?,
                    error: row.get(6)?,
                })
            })?
            .collect()
    }

    pub fn insert_service(&mut self, service: &Service) -> rusqlite::Result<()> {
        let restart_delay_ms = millis_to_sql(service.restart_delay_ms)?;
        let stop_grace_period_ms = millis_to_sql(service.stop_grace_period_ms)?;
        let log_max_size_mb = millis_to_sql(service.log_max_size_mb)?;
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO services (id,name,description,icon_path,group_id,execution_mode,executable,arguments_json,
                shell_command,working_directory,enabled,auto_start,restart_policy,restart_delay_ms,max_restart_attempts,
                priority,stop_grace_period_ms,sort_order,log_max_size_mb)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            params![service.id, service.name, service.description, service.icon_path, service.group_id,
                service.execution_mode, service.executable, serde_json::to_string(&service.arguments).unwrap_or_else(|_| "[]".into()),
                service.shell_command, service.working_directory, service.enabled, service.auto_start,
                service.restart_policy, restart_delay_ms, service.max_restart_attempts,
                service.priority, stop_grace_period_ms, service.sort_order, log_max_size_mb],
        )?;
        replace_environment(&tx, service)?;
        tx.commit()
    }

    pub fn update_service(&mut self, service: &Service) -> rusqlite::Result<()> {
        let restart_delay_ms = millis_to_sql(service.restart_delay_ms)?;
        let stop_grace_period_ms = millis_to_sql(service.stop_grace_period_ms)?;
        let log_max_size_mb = millis_to_sql(service.log_max_size_mb)?;
        let tx = self.connection.transaction()?;
        let changed = tx.execute(
            "UPDATE services SET name=?2,description=?3,icon_path=?4,group_id=?5,execution_mode=?6,
                executable=?7,arguments_json=?8,shell_command=?9,working_directory=?10,enabled=?11,auto_start=?12,
                restart_policy=?13,restart_delay_ms=?14,max_restart_attempts=?15,priority=?16,
                stop_grace_period_ms=?17,sort_order=?18,log_max_size_mb=?19,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
            params![service.id, service.name, service.description, service.icon_path, service.group_id,
                service.execution_mode, service.executable, serde_json::to_string(&service.arguments).unwrap_or_else(|_| "[]".into()),
                service.shell_command, service.working_directory, service.enabled, service.auto_start,
                service.restart_policy, restart_delay_ms, service.max_restart_attempts,
                service.priority, stop_grace_period_ms, service.sort_order, log_max_size_mb],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        replace_environment(&tx, service)?;
        tx.commit()
    }

    pub fn get_service(&self, id: &str) -> rusqlite::Result<Option<Service>> {
        let mut statement = self.connection.prepare("SELECT id,name,description,icon_path,group_id,execution_mode,
            executable,arguments_json,shell_command,working_directory,enabled,auto_start,restart_policy,restart_delay_ms,
            max_restart_attempts,priority,stop_grace_period_ms,created_at,updated_at,sort_order,log_max_size_mb FROM services WHERE id=?1")?;
        let service = statement.query_row([id], service_from_row).optional()?;
        service
            .map(|mut service| {
                service.environment = self.environment_for(id)?;
                Ok(service)
            })
            .transpose()
    }

    pub fn list_services(&self) -> rusqlite::Result<Vec<Service>> {
        let mut statement = self.connection.prepare("SELECT id,name,description,icon_path,group_id,execution_mode,
            executable,arguments_json,shell_command,working_directory,enabled,auto_start,restart_policy,restart_delay_ms,
            max_restart_attempts,priority,stop_grace_period_ms,created_at,updated_at,sort_order,log_max_size_mb
            FROM services ORDER BY sort_order,name COLLATE NOCASE")?;
        let mut services = statement
            .query_map([], service_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for service in &mut services {
            service.environment = self.environment_for(&service.id)?;
        }
        Ok(services)
    }

    pub fn delete_service(&mut self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM services WHERE id=?1", [id])?
            > 0)
    }

    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<serde_json::Value>> {
        self.connection
            .query_row(
                "SELECT value_json FROM app_settings WHERE key=?1",
                [key],
                |row| {
                    let value: String = row.get(0)?;
                    serde_json::from_str(&value).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })
                },
            )
            .optional()
    }

    pub fn set_setting(&mut self, key: &str, value: &serde_json::Value) -> rusqlite::Result<()> {
        let json = serde_json::to_string(value)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        self.connection.execute("INSERT INTO app_settings (key,value_json) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')", params![key, json])?;
        Ok(())
    }

    pub fn list_settings(
        &self,
    ) -> rusqlite::Result<std::collections::HashMap<String, serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT key,value_json FROM app_settings ORDER BY key")?;
        let rows = statement.query_map([], |row| {
            let key: String = row.get(0)?;
            let json: String = row.get(1)?;
            let value = serde_json::from_str(&json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok((key, value))
        })?;
        rows.collect()
    }

    fn environment_for(&self, id: &str) -> rusqlite::Result<Vec<EnvironmentVariable>> {
        let mut statement = self.connection.prepare(
            "SELECT key,value,is_secret FROM service_environment WHERE service_id=?1 ORDER BY key",
        )?;
        statement
            .query_map([id], |row| {
                Ok(EnvironmentVariable {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    is_secret: row.get::<_, i64>(2)? != 0,
                })
            })?
            .collect()
    }
}

fn replace_environment(tx: &Transaction<'_>, service: &Service) -> rusqlite::Result<()> {
    tx.execute(
        "DELETE FROM service_environment WHERE service_id=?1",
        [&service.id],
    )?;
    for variable in &service.environment {
        tx.execute("INSERT INTO service_environment (id,service_id,key,value,is_secret) VALUES (?1,?2,?3,?4,?5)",
            params![uuid::Uuid::new_v4().to_string(), service.id, variable.key, variable.value, variable.is_secret])?;
    }
    Ok(())
}

fn active_process_from_row(row: &Row<'_>) -> rusqlite::Result<ActiveProcessRecord> {
    Ok(ActiveProcessRecord {
        service_id: row.get(0)?,
        pid: u32::try_from(row.get::<_, i64>(1)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        process_group_id: u32::try_from(row.get::<_, i64>(2)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        process_start_time_s: u64::try_from(row.get::<_, i64>(3)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        executable_path: row.get(4)?,
        manager_session_id: row.get(5)?,
    })
}

fn service_from_row(row: &Row<'_>) -> rusqlite::Result<Service> {
    let arguments: String = row.get(7)?;
    let arguments = serde_json::from_str(&arguments).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(Service {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        icon_path: row.get(3)?,
        group_id: row.get(4)?,
        execution_mode: row.get(5)?,
        executable: row.get(6)?,
        arguments,
        shell_command: row.get(8)?,
        working_directory: row.get(9)?,
        environment: Vec::new(),
        enabled: row.get::<_, i64>(10)? != 0,
        auto_start: row.get::<_, i64>(11)? != 0,
        restart_policy: row.get(12)?,
        restart_delay_ms: millis_from_row(row, 13)?,
        max_restart_attempts: row.get(14)?,
        priority: row.get(15)?,
        stop_grace_period_ms: millis_from_row(row, 16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
        sort_order: row.get(19)?,
        log_max_size_mb: millis_from_row(row, 20)?,
    })
}

fn millis_to_sql(value: u64) -> rusqlite::Result<i64> {
    i64::try_from(value).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn millis_from_row(row: &Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(column)?;
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CreateService;

    fn service(name: &str) -> Service {
        Service::from_create(CreateService {
            name: name.into(),
            icon_path: None,
            executable: "node".into(),
            arguments: vec!["server.js".into(), "--port".into(), "3000".into()],
            working_directory: None,
            environment: vec![EnvironmentVariable {
                key: "MODE".into(),
                value: "test".into(),
                is_secret: false,
            }],
            description: Some("test service".into()),
            group_id: None,
            enabled: true,
            auto_start: false,
            restart_policy: "never".into(),
            restart_delay_ms: 1000,
            max_restart_attempts: Some(5),
            priority: "normal".into(),
            stop_grace_period_ms: 3000,
            log_max_size_mb: 10,
        })
        .expect("valid service")
    }

    #[test]
    fn clean_database_migrates_and_service_crud_preserves_fields() {
        let mut database = Database::open(":memory:").expect("open database");
        let version: i64 = database
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        assert_eq!(version, SCHEMA_VERSION);

        let service = service("API");
        database.insert_service(&service).expect("insert service");

        let loaded = database
            .get_service(&service.id)
            .expect("query service")
            .expect("service exists");
        assert_eq!(loaded.arguments, ["server.js", "--port", "3000"]);
        assert_eq!(loaded.environment[0].value, "test");
        assert_eq!(loaded.group_id, None);

        let mut updated = loaded;
        updated.name = "Renamed API".into();
        updated.restart_delay_ms = 42_000;
        database.update_service(&updated).expect("update service");
        let loaded = database.get_service(&service.id).unwrap().unwrap();
        assert_eq!(loaded.name, "Renamed API");
        assert_eq!(loaded.restart_delay_ms, 42_000);

        assert!(
            database
                .delete_service(&service.id)
                .expect("delete service")
        );
        assert!(database.get_service(&service.id).unwrap().is_none());
        let environment_count: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM service_environment", [], |row| {
                row.get(0)
            })
            .expect("count environment rows");
        assert_eq!(
            environment_count, 0,
            "service deletion cascades environment rows"
        );

        database
            .set_setting("theme", &serde_json::json!("light"))
            .expect("save setting");
        assert_eq!(
            database.get_setting("theme").unwrap(),
            Some(serde_json::json!("light"))
        );
    }

    #[test]
    fn services_survive_database_reopen_and_duplicate_names_are_rejected() {
        let path =
            std::env::temp_dir().join(format!("processhub-{}.sqlite3", uuid::Uuid::new_v4()));
        {
            let mut database = Database::open(&path).expect("open database");
            let original = service("Persistent API");
            database.insert_service(&original).expect("insert service");

            let duplicate = service("persistent api");
            assert!(database.insert_service(&duplicate).is_err());
        }
        {
            let database = Database::open(&path).expect("reopen database");
            let services = database.list_services().expect("list services");
            assert_eq!(services.len(), 1);
            assert_eq!(services[0].name, "Persistent API");
            assert_eq!(services[0].environment[0].key, "MODE");
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn run_history_keeps_the_latest_twenty_entries_and_cascades_on_service_delete() {
        let mut database = Database::open(":memory:").expect("open database");
        let service = service("History API");
        database.insert_service(&service).expect("insert service");

        for index in 0..25_u128 {
            database
                .insert_run_history(&RunHistory {
                    id: uuid::Uuid::new_v4().to_string(),
                    service_id: service.id.clone(),
                    started_at_ms: 1_000 + index,
                    ended_at_ms: 1_500 + index,
                    status: "exited".into(),
                    exit_code: Some(0),
                    error: None,
                })
                .expect("insert run history");
        }

        let history = database
            .list_run_history(&service.id)
            .expect("list run history");
        assert_eq!(history.len(), 20);
        assert_eq!(history[0].started_at_ms, 1_024);
        assert_eq!(history[19].started_at_ms, 1_005);

        database
            .delete_service(&service.id)
            .expect("delete service");
        assert!(
            database
                .list_run_history(&service.id)
                .expect("list deleted service history")
                .is_empty()
        );
    }

    #[test]
    fn version_one_database_migrates_run_history() {
        let connection = Connection::open_in_memory().expect("open v1 database");
        connection
            .execute_batch(
                "CREATE TABLE services (id TEXT PRIMARY KEY);
                 PRAGMA user_version = 1;",
            )
            .expect("create v1 schema");
        let mut database = Database { connection };
        database.migrate().expect("migrate v1 database");
        let version: i64 = database
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read migrated version");
        assert_eq!(version, SCHEMA_VERSION);
        let table_exists: bool = database
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='service_run_history')",
                [],
                |row| row.get(0),
            )
            .expect("check run history table");
        assert!(table_exists);
    }
}
