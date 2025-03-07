use std::collections::HashSet;

use log::info;
use nm_dbus::{NmApi, NmConnection, NmDeviceState};

use crate::{
    nm::connection::{
        create_index_for_nm_conns_by_name_type, iface_to_nm_connections,
        iface_type_to_nm, NM_SETTING_OVS_PORT_SETTING_NAME,
    },
    nm::device::create_index_for_nm_devs,
    nm::error::nm_error_to_nmstate,
    nm::profile::{
        activate_nm_profiles, deactivate_nm_profiles, delete_exist_profiles,
        get_exist_profile, save_nm_profiles, use_uuid_for_controller_reference,
    },
    nm::route::is_route_removed,
    Interface, InterfaceType, NetworkState, NmstateError, OvsBridgeInterface,
    RouteEntry,
};

pub(crate) fn nm_apply(
    add_net_state: &NetworkState,
    chg_net_state: &NetworkState,
    del_net_state: &NetworkState,
    cur_net_state: &NetworkState,
    des_net_state: &NetworkState,
    checkpoint: &str,
) -> Result<(), NmstateError> {
    let nm_api = NmApi::new().map_err(nm_error_to_nmstate)?;

    delete_net_state(&nm_api, del_net_state)?;
    apply_single_state(
        &nm_api,
        add_net_state,
        cur_net_state,
        des_net_state,
        checkpoint,
    )?;
    apply_single_state(
        &nm_api,
        chg_net_state,
        cur_net_state,
        des_net_state,
        checkpoint,
    )?;

    Ok(())
}

fn delete_net_state(
    nm_api: &NmApi,
    net_state: &NetworkState,
) -> Result<(), NmstateError> {
    // TODO: Should we remove inactive connections also?
    let all_nm_conns = nm_api.connections_get().map_err(nm_error_to_nmstate)?;

    let nm_conns_name_type_index =
        create_index_for_nm_conns_by_name_type(&all_nm_conns);
    let mut uuids_to_delete: HashSet<&str> = HashSet::new();

    for iface in &(net_state.interfaces.to_vec()) {
        if !iface.is_absent() {
            continue;
        }
        let nm_iface_type = iface_type_to_nm(&iface.iface_type())?;
        // Delete all existing connections for this interface
        if let Some(nm_conns) =
            nm_conns_name_type_index.get(&(iface.name(), &nm_iface_type))
        {
            for nm_conn in nm_conns {
                if let Some(uuid) = nm_conn.uuid() {
                    info!(
                        "Deleting NM connection for absent interface \
                            {}/{}: {}",
                        &iface.name(),
                        &iface.iface_type(),
                        uuid
                    );
                    uuids_to_delete.insert(uuid);
                }
                // Delete OVS port profile along with OVS Interface
                if iface.iface_type() == InterfaceType::OvsInterface {
                    // TODO: handle pre-exist OVS config using name instead of
                    // UUID for controller
                    if let Some(uuid) = nm_conn.controller() {
                        info!(
                            "Deleting NM OVS port connection {} \
                             for absent OVS interface {}",
                            uuid,
                            &iface.name(),
                        );
                        uuids_to_delete.insert(uuid);
                    }
                }
            }
        }
    }

    for uuid in &uuids_to_delete {
        nm_api
            .connection_delete(uuid)
            .map_err(nm_error_to_nmstate)?;
    }

    delete_orphan_ports(nm_api, &uuids_to_delete)?;
    delete_unmanged_virtual_interface_as_desired(nm_api, net_state)?;
    Ok(())
}

fn apply_single_state(
    nm_api: &NmApi,
    net_state: &NetworkState,
    cur_net_state: &NetworkState,
    des_net_state: &NetworkState,
    checkpoint: &str,
) -> Result<(), NmstateError> {
    let mut nm_conns_to_activate: Vec<NmConnection> = Vec::new();

    let exist_nm_conns =
        nm_api.connections_get().map_err(nm_error_to_nmstate)?;
    let nm_acs = nm_api
        .active_connections_get()
        .map_err(nm_error_to_nmstate)?;
    let nm_ac_uuids: Vec<&str> =
        nm_acs.iter().map(|nm_ac| &nm_ac.uuid as &str).collect();
    let activated_nm_conns: Vec<&NmConnection> = exist_nm_conns
        .iter()
        .filter(|c| {
            if let Some(uuid) = c.uuid() {
                nm_ac_uuids.contains(&uuid)
            } else {
                false
            }
        })
        .collect();

    let ifaces = net_state.interfaces.to_vec();

    for iface in ifaces.iter() {
        if iface.iface_type() != InterfaceType::Unknown && iface.is_up() {
            let mut ctrl_iface: Option<&Interface> = None;
            if let Some(ctrl_iface_name) = &iface.base_iface().controller {
                if let Some(ctrl_type) = &iface.base_iface().controller_type {
                    ctrl_iface = des_net_state
                        .interfaces
                        .get_iface(ctrl_iface_name, ctrl_type.clone());
                }
            }
            let mut routes: Vec<&RouteEntry> = Vec::new();
            if let Some(config_routes) = net_state.routes.config.as_ref() {
                for route in config_routes {
                    if let Some(i) = route.next_hop_iface.as_ref() {
                        if i == iface.name() {
                            routes.push(route);
                        }
                    }
                }
            }
            for nm_conn in iface_to_nm_connections(
                iface,
                ctrl_iface,
                &exist_nm_conns,
                &nm_ac_uuids,
            )? {
                nm_conns_to_activate.push(nm_conn);
            }
        }
    }
    let nm_conns_to_deactivate = ifaces
        .into_iter()
        .filter(|iface| iface.is_down())
        .filter_map(|iface| {
            get_exist_profile(
                &exist_nm_conns,
                &iface.base_iface().name,
                &iface.base_iface().iface_type,
                &nm_ac_uuids,
            )
        })
        .collect::<Vec<_>>();

    let mut ovs_br_ifaces: Vec<&OvsBridgeInterface> = Vec::new();
    for iface in net_state.interfaces.user_ifaces.values() {
        if let Interface::OvsBridge(ref br_iface) = iface {
            ovs_br_ifaces.push(br_iface);
        }
    }

    use_uuid_for_controller_reference(
        &mut nm_conns_to_activate,
        &des_net_state.interfaces.user_ifaces,
        &cur_net_state.interfaces.user_ifaces,
        &exist_nm_conns,
    )?;

    let nm_conns_to_deactivate_first = gen_nm_conn_need_to_deactivate_first(
        nm_conns_to_activate.as_slice(),
        activated_nm_conns.as_slice(),
    );
    deactivate_nm_profiles(
        nm_api,
        nm_conns_to_deactivate_first.as_slice(),
        checkpoint,
    )?;
    save_nm_profiles(nm_api, nm_conns_to_activate.as_slice(), checkpoint)?;
    delete_exist_profiles(nm_api, &exist_nm_conns, &nm_conns_to_activate)?;

    activate_nm_profiles(nm_api, nm_conns_to_activate.as_slice(), checkpoint)?;
    deactivate_nm_profiles(
        nm_api,
        nm_conns_to_deactivate.as_slice(),
        checkpoint,
    )?;
    Ok(())
}

fn delete_unmanged_virtual_interface_as_desired(
    nm_api: &NmApi,
    net_state: &NetworkState,
) -> Result<(), NmstateError> {
    let nm_devs = nm_api.devices_get().map_err(nm_error_to_nmstate)?;
    let nm_devs_indexed = create_index_for_nm_devs(&nm_devs);
    // Delete unmanaged software(virtual) interface
    for iface in &(net_state.interfaces.to_vec()) {
        if !iface.is_absent() {
            continue;
        }
        if iface.is_virtual() {
            if let Some(nm_dev) = nm_devs_indexed.get(&(
                iface.name().to_string(),
                iface.iface_type().to_string(),
            )) {
                if nm_dev.state == NmDeviceState::Unmanaged {
                    info!(
                        "Deleting NM unmanaged interface {}/{}: {}",
                        &iface.name(),
                        &iface.iface_type(),
                        &nm_dev.obj_path
                    );
                    nm_api
                        .device_delete(&nm_dev.obj_path)
                        .map_err(nm_error_to_nmstate)?;
                }
            }
        }
    }
    Ok(())
}

// If any connection still referring to deleted UUID, we should delete it also
fn delete_orphan_ports(
    nm_api: &NmApi,
    uuids_deleted: &HashSet<&str>,
) -> Result<(), NmstateError> {
    let mut uuids_to_delete = Vec::new();
    let all_nm_conns = nm_api.connections_get().map_err(nm_error_to_nmstate)?;
    for nm_conn in &all_nm_conns {
        if nm_conn.iface_type() != Some(NM_SETTING_OVS_PORT_SETTING_NAME) {
            continue;
        }
        if let Some(ctrl_uuid) = nm_conn.controller() {
            if uuids_deleted.contains(ctrl_uuid) {
                if let Some(uuid) = nm_conn.uuid() {
                    info!(
                        "Deleting NM orphan profile {}/{}: {}",
                        nm_conn.iface_name().unwrap_or(""),
                        nm_conn.iface_type().unwrap_or(""),
                        uuid
                    );
                    uuids_to_delete.push(uuid);
                }
            }
        }
    }
    for uuid in &uuids_to_delete {
        nm_api
            .connection_delete(uuid)
            .map_err(nm_error_to_nmstate)?;
    }
    Ok(())
}

// NM has problem on remove routes, we need to deactivate it first
//  https://bugzilla.redhat.com/1837254
fn gen_nm_conn_need_to_deactivate_first<'a>(
    nm_conns_to_activate: &[NmConnection],
    activated_nm_conns: &[&'a NmConnection],
) -> Vec<&'a NmConnection> {
    let mut ret: Vec<&NmConnection> = Vec::new();
    for nm_conn in nm_conns_to_activate {
        if let Some(uuid) = nm_conn.uuid() {
            if let Some(activated_nm_con) =
                activated_nm_conns.iter().find(|c| {
                    if let Some(cur_uuid) = c.uuid() {
                        cur_uuid == uuid
                    } else {
                        false
                    }
                })
            {
                if is_route_removed(nm_conn, activated_nm_con) {
                    ret.push(activated_nm_con);
                }
            }
        }
    }
    ret
}
