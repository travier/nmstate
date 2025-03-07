use std::convert::TryFrom;

use log::warn;
use nm_dbus::NmConnection;

use crate::{
    nm::connection::gen_nm_conn_setting, BaseInterface, Interface,
    InterfaceType, NmstateError, OvsBridgeBondConfig, OvsBridgeBondMode,
    OvsBridgeBondPortConfig, OvsBridgeConfig, OvsBridgeInterface,
    OvsBridgeOptions, OvsBridgePortConfig, UnknownInterface,
};

pub(crate) fn nm_ovs_bridge_conf_get(
    nm_conn: &NmConnection,
    port_nm_conns: Option<&[&NmConnection]>,
) -> Result<OvsBridgeConfig, NmstateError> {
    let mut ovs_br_conf = OvsBridgeConfig::new();
    if let Some(nm_ovs_setting) = &nm_conn.ovs_bridge {
        let mut br_opts = OvsBridgeOptions::new();
        // The DBUS interface of NM does not return default values
        // We set default values to be consistent with old nmstate behavior
        br_opts.stp = match nm_ovs_setting.stp {
            Some(n) => Some(n),
            None => Some(false),
        };
        br_opts.rstp = match nm_ovs_setting.rstp {
            Some(n) => Some(n),
            None => Some(false),
        };
        br_opts.mcast_snooping_enable =
            match nm_ovs_setting.mcast_snooping_enable {
                Some(n) => Some(n),
                None => Some(false),
            };
        br_opts.fail_mode = match nm_ovs_setting.fail_mode.as_ref() {
            Some(m) => Some(m.to_string()),
            None => Some("".to_string()),
        };
        ovs_br_conf.options = Some(br_opts);
        if let Some(port_nm_conns) = port_nm_conns {
            ovs_br_conf.ports =
                Some(nm_ovs_bridge_conf_port_get(port_nm_conns));
        }
    }
    Ok(ovs_br_conf)
}

fn nm_ovs_bridge_conf_port_get(
    nm_conns: &[&NmConnection],
) -> Vec<OvsBridgePortConfig> {
    let mut ret = Vec::new();
    for nm_conn in nm_conns {
        if nm_conn.iface_type() == Some("ovs-port") {
            let nm_ovs_iface_conns = get_nm_ovs_iface_conns(nm_conn, nm_conns);
            match nm_ovs_iface_conns.len() {
                d if d > 1 => {
                    if let Some(p) = get_ovs_port_config_for_bond(
                        nm_conn,
                        &nm_ovs_iface_conns,
                    ) {
                        ret.push(p);
                    }
                }
                1 => {
                    if let Some(p) =
                        get_ovs_port_config_for_iface(nm_ovs_iface_conns[0])
                    {
                        ret.push(p);
                    }
                }
                _ => (),
            };
        }
    }
    ret
}

fn get_ovs_port_config_for_bond(
    nm_ovs_port_conn: &NmConnection,
    nm_ovs_iface_conns: &[&NmConnection],
) -> Option<OvsBridgePortConfig> {
    let mut port_conf = OvsBridgePortConfig::new();
    if let Some(n) = nm_ovs_port_conn.iface_name() {
        port_conf.name = n.to_string();
    } else {
        return None;
    }
    let mut ovs_bond_conf = OvsBridgeBondConfig::new();

    let nm_port_set = if let Some(s) = &nm_ovs_port_conn.ovs_port {
        s
    } else {
        return None;
    };

    ovs_bond_conf.mode = nm_port_set.mode.as_ref().and_then(|nm_mode| {
        if let Ok(m) = OvsBridgeBondMode::try_from(nm_mode.as_str()) {
            Some(m)
        } else {
            warn!("Unsupported OVS bond mode {}", nm_mode);
            None
        }
    });

    ovs_bond_conf.bond_downdelay = nm_port_set.down_delay;
    ovs_bond_conf.bond_updelay = nm_port_set.up_delay;
    let mut ovs_iface_confs = Vec::new();

    for nm_ovs_iface_conn in nm_ovs_iface_conns {
        if let Some(name) = nm_ovs_iface_conn.iface_name() {
            ovs_iface_confs.push(OvsBridgeBondPortConfig {
                name: name.to_string(),
            })
        }
    }

    ovs_bond_conf.ports = Some(ovs_iface_confs);
    port_conf.bond = Some(ovs_bond_conf);

    Some(port_conf)
}

fn get_ovs_port_config_for_iface(
    nm_conn: &NmConnection,
) -> Option<OvsBridgePortConfig> {
    if let Some(name) = nm_conn.iface_name() {
        let mut port_conf = OvsBridgePortConfig::new();
        port_conf.name = name.to_string();
        Some(port_conf)
    } else {
        None
    }
}

fn get_nm_ovs_iface_conns<'a>(
    nm_ovs_port_conn: &'a NmConnection,
    nm_conns: &'a [&'a NmConnection],
) -> Vec<&'a NmConnection> {
    let mut ret = Vec::new();
    let uuid = if let Some(n) = nm_ovs_port_conn.uuid() {
        n
    } else {
        return ret;
    };
    let name = if let Some(n) = nm_ovs_port_conn.iface_name() {
        n
    } else {
        return ret;
    };
    for nm_conn in nm_conns {
        if nm_conn.controller_type() == Some("ovs-port")
            && (nm_conn.controller() == Some(uuid)
                || nm_conn.controller() == Some(name))
        {
            ret.push(nm_conn)
        }
    }
    ret
}

pub(crate) fn create_ovs_port_nm_conn(
    br_name: &str,
    port_conf: &OvsBridgePortConfig,
    exist_nm_conn: Option<&NmConnection>,
) -> Result<NmConnection, NmstateError> {
    let mut nm_conn = exist_nm_conn.cloned().unwrap_or_default();
    let mut base_iface = BaseInterface::new();
    base_iface.name = port_conf.name.clone();
    base_iface.iface_type = InterfaceType::Other("ovs-port".to_string());
    base_iface.controller = Some(br_name.to_string());
    base_iface.controller_type = Some(InterfaceType::OvsBridge);
    let mut iface = UnknownInterface::new();
    iface.base = base_iface;
    gen_nm_conn_setting(&Interface::Unknown(iface), &mut nm_conn)?;

    let mut nm_ovs_port_set =
        nm_conn.ovs_port.as_ref().cloned().unwrap_or_default();
    if let Some(bond_conf) = &port_conf.bond {
        if let Some(bond_mode) = &bond_conf.mode {
            nm_ovs_port_set.mode = Some(format!("{}", bond_mode));
        }

        if let Some(bond_downdelay) = bond_conf.bond_downdelay {
            nm_ovs_port_set.down_delay = Some(bond_downdelay);
        }

        if let Some(bond_updelay) = bond_conf.bond_updelay {
            nm_ovs_port_set.up_delay = Some(bond_updelay);
        }
    }
    nm_conn.ovs_port = Some(nm_ovs_port_set);
    Ok(nm_conn)
}

pub(crate) fn get_ovs_port_name(
    ovs_br_iface: &OvsBridgeInterface,
    ovs_iface_name: &str,
) -> Option<String> {
    for port_conf in ovs_br_iface.port_confs() {
        if let Some(bond_conf) = &port_conf.bond {
            for bond_port_name in bond_conf.ports() {
                if bond_port_name == ovs_iface_name {
                    return Some(port_conf.name.as_str().to_string());
                }
            }
        } else if ovs_iface_name == port_conf.name {
            return Some(ovs_iface_name.to_string());
        }
    }
    None
}

pub(crate) fn gen_nm_ovs_br_setting(
    ovs_br_iface: &OvsBridgeInterface,
    nm_conn: &mut NmConnection,
) {
    let mut nm_ovs_br_set =
        nm_conn.ovs_bridge.as_ref().cloned().unwrap_or_default();

    if let Some(br_conf) = &ovs_br_iface.bridge {
        if let Some(br_opts) = &br_conf.options {
            nm_ovs_br_set.stp = br_opts.stp;
            nm_ovs_br_set.rstp = br_opts.rstp;
            nm_ovs_br_set.mcast_snooping_enable = br_opts.mcast_snooping_enable;
            if let Some(fail_mode) = &br_opts.fail_mode {
                if !fail_mode.is_empty() {
                    nm_ovs_br_set.fail_mode = Some(fail_mode.to_string());
                }
            }
        }
    }
    nm_conn.ovs_bridge = Some(nm_ovs_br_set);
}

pub(crate) fn gen_nm_ovs_iface_setting(nm_conn: &mut NmConnection) {
    let mut nm_ovs_iface_set =
        nm_conn.ovs_iface.as_ref().cloned().unwrap_or_default();
    nm_ovs_iface_set.iface_type = Some("internal".to_string());
    nm_conn.ovs_iface = Some(nm_ovs_iface_set);
}
