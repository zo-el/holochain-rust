use crate::{NetworkingType, NEW_RELIC_LICENSE_KEY};
use cli;
use colored::*;
use error::DefaultResult;
use holochain_common::env_vars::EnvVar;
use holochain_conductor_lib::{
    conductor::{mount_conductor_from_config, Conductor, CONDUCTOR},
    config::*,
    happ_bundle::HappBundle,
    key_loaders::{test_keystore, test_keystore_loader},
    keystore::PRIMARY_KEYBUNDLE_ID,
    logger::LogRules,
};
use holochain_core_types::agent::AgentId;
use holochain_net::sim2h_worker::Sim2hConfig;
use holochain_persistence_api::cas::content::AddressableContent;
use std::{fs, path::PathBuf};

pub enum Networking {
    Sim2h(String),
}

/// Starts a minimal configuration Conductor with the current application running
#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
pub fn run(
    dna_path: PathBuf,
    package: bool,
    port: u16,
    interface_type: String,
    conductor_config: Configuration,
) -> DefaultResult<()> {
    if package {
        cli::package(dna_path, json!({}))?;
    }

    mount_conductor_from_config(conductor_config);
    let mut conductor_guard = CONDUCTOR.lock().unwrap();
    let conductor = conductor_guard.as_mut().expect("Conductor must be mounted");
    conductor.key_loader = test_keystore_loader();

    conductor
        .boot_from_config()
        .map_err(|err| format_err!("{}", err))?;

    conductor.start_all_interfaces();
    conductor.start_all_instances()?;
    conductor
        .start_all_static_servers()
        .map_err(|e| failure::err_msg(e))?;

    println!(
        "Holochain development conductor started. Running {} server on port {}",
        interface_type, port
    );
    println!("Type 'exit' to stop the conductor and exit the program");

    let mut rl = rustyline::Editor::<()>::new();

    loop {
        let readline = rl.readline("hc> ")?;

        match readline.as_str().trim() {
            "exit" => break,
            other if !other.is_empty() => eprintln!(
                "command {} not recognized. Available commands are: exit",
                other.red().bold()
            ),
            _ => continue,
        }
    }

    Ok(())
}

#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
pub fn get_interface_type_string(given_type: String) -> String {
    // note that this behaviour is documented within
    // holochain_common::env_vars module and should be updated
    // if this logic changes
    // The environment variable overrides the CLI flag
    EnvVar::Interface.value().ok().unwrap_or_else(|| given_type)
}

#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
pub fn hc_run_configuration(
    dna_path: &PathBuf,
    port: u16,
    persist: bool,
    networked: Option<Networking>,
    interface_type: &String,
    logging: bool,
    agent_name: String,
) -> DefaultResult<Configuration> {
    Ok(Configuration {
        agents: vec![agent_configuration(agent_name)],
        dnas: vec![dna_configuration(&dna_path)],
        instances: vec![instance_configuration(storage_configuration(persist)?)],
        interfaces: vec![interface_configuration(&interface_type, port)?],
        network: networking_configuration(networked),
        logger: logger_configuration(logging),
        ..Default::default()
    })
}

#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
pub fn hc_run_bundle_configuration(
    bundle: &HappBundle,
    port: u16,
    persist: bool,
    networked: Option<Networking>,
    logging: bool,
    agent_name: String,
) -> DefaultResult<Configuration> {
    bundle
        .build_conductor_config(
            port,
            agent_configuration(agent_name),
            storage_configuration(persist)?,
            networking_configuration(networked),
            logger_configuration(logging),
        )
        .map_err(|e| failure::err_msg(e))
}

// AGENT
pub(crate) const AGENT_NAME_DEFAULT: &str = "testAgent";
const AGENT_CONFIG_ID: &str = "hc-run-agent";

#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
fn agent_configuration(agent_name: String) -> AgentConfiguration {
    // note that this behaviour is documented within
    // holochain_common::env_vars module and should be updated
    // if this logic changes
    let agent_name = EnvVar::Agent.value().ok().unwrap_or_else(|| agent_name);
    let mut keystore = test_keystore(&agent_name);
    let pub_key = keystore
        .get_keybundle(PRIMARY_KEYBUNDLE_ID)
        .expect("should be able to get keybundle")
        .get_id();
    let agent_id = AgentId::new(&agent_name, pub_key);
    AgentConfiguration {
        id: AGENT_CONFIG_ID.into(),
        name: agent_id.nick,
        public_address: agent_id.pub_sign_key,
        keystore_file: agent_name,
        holo_remote_key: None,
        test_agent: Some(true),
    }
}

// DNA
const DNA_CONFIG_ID: &str = "hc-run-dna";

#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
fn dna_configuration(dna_path: &PathBuf) -> DnaConfiguration {
    let dna = Conductor::load_dna(dna_path).unwrap_or_else(|_| {
        panic!(
            "Could not load DNA file {}",
            dna_path.to_str().expect("No DNA file path given")
        )
    });
    DnaConfiguration {
        id: DNA_CONFIG_ID.into(),
        file: dna_path
            .to_str()
            .expect("Expected DNA path to be valid unicode")
            .to_string(),
        hash: dna.address().to_string(),
        uuid: None,
    }
}

// STORAGE
const LOCAL_STORAGE_PATH: &str = ".hc";

#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
fn storage_configuration(persist: bool) -> DefaultResult<StorageConfiguration> {
    if persist {
        fs::create_dir_all(LOCAL_STORAGE_PATH)?;

        Ok(StorageConfiguration::Pickle {
            path: LOCAL_STORAGE_PATH.into(),
        })
    } else {
        Ok(StorageConfiguration::Memory)
    }
}

// INSTANCE
const INSTANCE_CONFIG_ID: &str = "test-instance";

#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
fn instance_configuration(storage: StorageConfiguration) -> InstanceConfiguration {
    InstanceConfiguration {
        id: INSTANCE_CONFIG_ID.into(),
        dna: DNA_CONFIG_ID.into(),
        agent: AGENT_CONFIG_ID.into(),
        storage,
    }
}

// INTERFACE
const INTERFACE_CONFIG_ID: &str = "websocket-interface";

#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
fn interface_configuration(
    interface_type: &String,
    port: u16,
) -> DefaultResult<InterfaceConfiguration> {
    let driver = if interface_type == &String::from("websocket") {
        InterfaceDriver::Websocket { port }
    } else if interface_type == &String::from("http") {
        InterfaceDriver::Http { port }
    } else {
        return Err(format_err!("unknown interface type: {}", interface_type));
    };

    Ok(InterfaceConfiguration {
        id: INTERFACE_CONFIG_ID.into(),
        driver,
        admin: true,
        instances: vec![InstanceReferenceConfiguration {
            id: INSTANCE_CONFIG_ID.into(),
            alias: None,
        }],
        choose_free_port: None,
    })
}

// LOGGER
#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
fn logger_configuration(logging: bool) -> LoggerConfiguration {
    // temporary log rules, should come from a configuration
    LoggerConfiguration {
        logger_level: "debug".to_string(),
        rules: if logging {
            LogRules::default()
        } else {
            LogRules::new()
        },
        state_dump: true,
    }
}

// NETWORKING
#[holochain_tracing_macros::newrelic_autotrace(HOLOCHAIN_CLI)]
fn networking_configuration(networked: Option<Networking>) -> Option<NetworkConfig> {
    // create an n3h network config if the --networked flag is set
    let networked = match networked {
        Some(n) => n,
        None => return None,
    };

    match networked {
        Networking::Sim2h(sim2h_url) => Some(NetworkConfig::Sim2h(Sim2hConfig { sim2h_url })),
    }
}

impl Networking {
    pub fn new(networking_type: NetworkingType, sim2h_url: String) -> Self {
        match networking_type {
            NetworkingType::Sim2h => Self::Sim2h(sim2h_url),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate tempfile;
    // use crate::cli::init::{init, tests::gen_dir};
    // use assert_cmd::prelude::*;
    // use std::{env, process::Command, path::PathBuf};
    use self::tempfile::tempdir;
    use super::Networking;
    use holochain_conductor_lib::config::*;
    use holochain_core_types::dna::Dna;
    use holochain_net::sim2h_worker::Sim2hConfig;
    use holochain_persistence_api::cas::content::AddressableContent;
    use std::fs::{create_dir, File};

    #[test]
    // flagged as broken for:
    // 1. taking 60+ seconds
    // 2. test doesn't take into account dynamic folder for package name
    // 3. test is broken in regard to reading an agent key
    #[cfg(feature = "broken-tests")]
    fn test_run() {
        let temp_dir = gen_dir();
        let temp_dir_path = temp_dir.path();
        let temp_dir_path_buf = temp_dir_path.to_path_buf();

        let mut run_cmd = Command::main_binary().unwrap();
        let mut run2_cmd = Command::main_binary().unwrap();

        let _ = init(&temp_dir_path_buf);

        assert!(env::set_current_dir(&temp_dir_path).is_ok());

        let output = run_cmd
            .args(&["run", "--package"])
            .output()
            .expect("should run");
        assert_eq!(format!("{:?}",output),"Output { status: ExitStatus(ExitStatus(256)), stdout: \"\\u{1b}[1;32mCreated\\u{1b}[0m dna package file at \\\"x.dna.json\\\"\\nStarting instance \\\"test-instance\\\"...\\nHolochain development conductor started. Running websocket server on port 8888\\nType \\\'exit\\\' to stop the conductor and exit the program\\n\", stderr: \"Error: EOF\\n\" }");

        let output = run2_cmd
            .args(&["run", "--interface", "http"])
            .output()
            .expect("should run");
        assert_eq!(format!("{:?}",output),"Output { status: ExitStatus(ExitStatus(256)), stdout: \"Starting instance \\\"test-instance\\\"...\\nHolochain development conductor started. Running http server on port 8888\\nType \\\'exit\\\' to stop the conductor and exit the program\\n\", stderr: \"Error: EOF\\n\" }");
    }

    #[test]
    fn test_agent_configuration() {
        let agent = super::agent_configuration(super::AGENT_NAME_DEFAULT.to_string());
        assert_eq!(
            agent,
            AgentConfiguration {
                id: "hc-run-agent".to_string(),
                name: "testAgent".to_string(),
                public_address: "HcScjN8wBwrn3tuyg89aab3a69xsIgdzmX5P9537BqQZ5A7TEZu7qCY4Xzzjhma"
                    .to_string(),
                keystore_file: "testAgent".to_string(),
                holo_remote_key: None,
                test_agent: Some(true),
            },
        );
    }

    #[test]
    fn test_dna_configuration() {
        let dna = Dna::new();
        let temp_path = tempdir()
            .expect("Could not get tempdir")
            .path()
            .join("test_dna.json");
        create_dir(temp_path.parent().unwrap()).expect("Could not create temporary directory");
        let out_file = File::create(&temp_path).expect("Could not create temp file for test DNA");
        serde_json::to_writer_pretty(&out_file, &dna).expect("Could not write test DNA to file");

        let dna_config = super::dna_configuration(&temp_path);
        assert_eq!(
            dna_config,
            DnaConfiguration {
                id: "hc-run-dna".to_string(),
                file: temp_path.to_str().unwrap().to_string(),
                hash: dna.address().to_string(),
                uuid: Default::default(),
            }
        )
    }

    #[test]
    fn test_storage_configuration() {
        let storage = super::storage_configuration(false).unwrap();
        assert_eq!(storage, StorageConfiguration::Memory);

        let persist_store = super::storage_configuration(true).unwrap();
        assert_eq!(
            persist_store,
            StorageConfiguration::Pickle {
                path: ".hc".to_string()
            }
        );
    }

    #[test]
    fn test_instance_configuration() {
        let storage = super::storage_configuration(false).unwrap();
        let instance = super::instance_configuration(storage);
        assert_eq!(
            instance,
            InstanceConfiguration {
                id: "test-instance".to_string(),
                dna: "hc-run-dna".to_string(),
                agent: "hc-run-agent".to_string(),
                storage: StorageConfiguration::Memory,
            }
        )
    }

    #[test]
    fn test_interface_configuration() {
        let http_interface = super::interface_configuration(&"http".to_string(), 4444).unwrap();
        assert_eq!(
            http_interface,
            InterfaceConfiguration {
                id: "websocket-interface".to_string(),
                driver: InterfaceDriver::Http { port: 4444 },
                admin: true,
                instances: vec![InstanceReferenceConfiguration {
                    id: "test-instance".to_string(),
                    alias: None,
                }],
                choose_free_port: None,
            }
        );

        let websocket_interface =
            super::interface_configuration(&"websocket".to_string(), 5555).unwrap();
        assert_eq!(
            websocket_interface,
            InterfaceConfiguration {
                id: "websocket-interface".to_string(),
                driver: InterfaceDriver::Websocket { port: 5555 },
                admin: true,
                instances: vec![InstanceReferenceConfiguration {
                    id: "test-instance".to_string(),
                    alias: None,
                }],
                choose_free_port: None,
            }
        );

        let invalid_type = super::interface_configuration(&"funny".to_string(), 4444);
        assert!(invalid_type.is_err());
    }

    #[test]
    fn test_networking_configuration() {
        let networking =
            super::networking_configuration(Some(Networking::Sim2h("wss://localhost:9000".into())));
        assert_eq!(
            networking,
            Some(NetworkConfig::Sim2h(Sim2hConfig {
                sim2h_url: "wss://localhost:9000".into()
            }))
        );

        let no_networking = super::networking_configuration(None);
        assert!(no_networking.is_none());
    }
}
