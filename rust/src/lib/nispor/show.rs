// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::{
    nispor::{
        base_iface::np_iface_to_base_iface,
        bond::{append_bond_port_config, np_bond_to_nmstate},
        dns::get_dns,
        error::np_error_to_nmstate,
        ethernet::np_ethernet_to_nmstate,
        hostname::get_hostname_state,
        hsr::np_hsr_to_nmstate,
        infiniband::np_ib_to_nmstate,
        ipvlan::np_ipvlan_to_nmstate,
        linux_bridge::{append_bridge_port_config, np_bridge_to_nmstate},
        mac_vlan::{np_mac_vlan_to_nmstate, np_mac_vtap_to_nmstate},
        macsec::np_macsec_to_nmstate,
        route::get_routes,
        route_rule::get_route_rules,
        veth::np_veth_to_nmstate,
        vlan::np_vlan_to_nmstate,
        vrf::np_vrf_to_nmstate,
        vxlan::np_vxlan_to_nmstate,
    },
    DummyInterface, Interface, InterfaceType, Interfaces, LoopbackInterface,
    NetworkState, NmstateError, OvsInterface, UnknownInterface, XfrmInterface,
};

// Only report DNS config when `kernel_only: true`
pub(crate) async fn nispor_retrieve(
    running_config_only: bool,
    kernel_only: bool,
) -> Result<NetworkState, NmstateError> {
    let mut net_state = NetworkState {
        hostname: get_hostname_state(),
        ..Default::default()
    };
    let mut filter = nispor::NetStateFilter::default();
    // Do not query routes in order to prevent BGP routes consuming too much CPU
    // time, we let `get_routes()` do the query by itself.
    filter.route = None;
    let np_state = nispor::NetState::retrieve_with_filter_async(&filter)
        .await
        .map_err(np_error_to_nmstate)?;

    for (_, np_iface) in np_state.ifaces.iter() {
        // The `ovs-system` is reserved for OVS kernel datapath
        if np_iface.name == "ovs-system" {
            continue;
        }
        // The `ovs-netdev` is reserved for OVS netdev datapath
        if np_iface.name == "ovs-netdev" {
            continue;
        }
        // The vti interface is reserved for Ipsec
        if np_iface.iface_type == nispor::IfaceType::Other("Vti".into()) {
            continue;
        }

        let base_iface = np_iface_to_base_iface(np_iface, running_config_only);
        let iface = match &base_iface.iface_type {
            InterfaceType::LinuxBridge => {
                let mut br_iface = np_bridge_to_nmstate(np_iface, base_iface)?;
                let mut port_np_ifaces = Vec::new();
                for port_name in br_iface.ports().unwrap_or_default() {
                    if let Some(p) = np_state.ifaces.get(port_name) {
                        port_np_ifaces.push(p)
                    }
                }
                append_bridge_port_config(
                    &mut br_iface,
                    np_iface,
                    port_np_ifaces,
                );
                Interface::LinuxBridge(Box::new(br_iface))
            }
            InterfaceType::Bond => {
                let mut bond_iface = np_bond_to_nmstate(np_iface, base_iface);
                let mut port_np_ifaces = Vec::new();
                for port_name in bond_iface.ports().unwrap_or_default() {
                    if let Some(p) = np_state.ifaces.get(port_name) {
                        port_np_ifaces.push(p)
                    }
                }
                append_bond_port_config(&mut bond_iface, port_np_ifaces);
                Interface::Bond(Box::new(bond_iface))
            }
            InterfaceType::Ethernet => Interface::Ethernet(Box::new(
                np_ethernet_to_nmstate(np_iface, base_iface),
            )),
            InterfaceType::Hsr => Interface::Hsr(Box::new(np_hsr_to_nmstate(
                np_iface, base_iface,
            ))),
            InterfaceType::Veth => Interface::Ethernet(Box::new(
                np_veth_to_nmstate(np_iface, base_iface),
            )),
            InterfaceType::Vlan => Interface::Vlan(Box::new(
                np_vlan_to_nmstate(np_iface, base_iface),
            )),
            InterfaceType::Vxlan => Interface::Vxlan(Box::new(
                np_vxlan_to_nmstate(np_iface, base_iface),
            )),
            InterfaceType::Dummy => Interface::Dummy({
                let mut iface = DummyInterface::new();
                iface.base = base_iface;
                Box::new(iface)
            }),
            InterfaceType::OvsInterface => Interface::OvsInterface({
                let mut iface = OvsInterface::new();
                iface.base = base_iface;
                Box::new(iface)
            }),
            InterfaceType::MacVlan => Interface::MacVlan(Box::new(
                np_mac_vlan_to_nmstate(np_iface, base_iface),
            )),
            InterfaceType::MacVtap => Interface::MacVtap(Box::new(
                np_mac_vtap_to_nmstate(np_iface, base_iface),
            )),
            InterfaceType::Vrf => Interface::Vrf(Box::new(np_vrf_to_nmstate(
                np_iface, base_iface,
            ))),
            InterfaceType::InfiniBand => {
                // We don't support HFI interface which contains PKEY but no
                // parent.
                if base_iface.name.starts_with("hfi1") {
                    log::debug!(
                        "Ignoring unsupported HFI interface {}",
                        base_iface.name
                    );
                    continue;
                }
                Interface::InfiniBand(Box::new(np_ib_to_nmstate(
                    np_iface, base_iface,
                )))
            }
            InterfaceType::Loopback => {
                Interface::Loopback(Box::new(LoopbackInterface {
                    base: base_iface,
                }))
            }
            InterfaceType::MacSec => Interface::MacSec(Box::new(
                np_macsec_to_nmstate(np_iface, base_iface),
            )),
            InterfaceType::Xfrm => {
                let mut iface = XfrmInterface::new();
                iface.base = base_iface;
                Interface::Xfrm(Box::new(iface))
            }
            InterfaceType::IpVlan => Interface::IpVlan(Box::new(
                np_ipvlan_to_nmstate(np_iface, base_iface),
            )),
            _ => {
                log::debug!(
                    "Got unsupported interface {} type {:?}",
                    np_iface.name,
                    np_iface.iface_type
                );
                Interface::Unknown({
                    let mut iface = UnknownInterface::new();
                    iface.base = base_iface;
                    Box::new(iface)
                })
            }
        };
        net_state.append_interface_data(iface);
    }
    set_controller_type(&mut net_state.interfaces);
    net_state.routes = get_routes(running_config_only).await;
    net_state.rules = get_route_rules(&np_state.rules, running_config_only);
    if kernel_only {
        net_state.dns = get_dns();
    }
    Ok(net_state)
}

fn set_controller_type(ifaces: &mut Interfaces) {
    let mut ctrl_to_type: HashMap<String, InterfaceType> = HashMap::new();
    for iface in ifaces.to_vec() {
        if iface.is_controller() {
            ctrl_to_type
                .insert(iface.name().to_string(), iface.iface_type().clone());
        }
    }
    for iface in ifaces.kernel_ifaces.values_mut() {
        if let Some(ctrl) = iface.base_iface().controller.as_ref() {
            if let Some(ctrl_type) = ctrl_to_type.get(ctrl) {
                iface.base_iface_mut().controller_type =
                    Some(ctrl_type.clone());
            }
        }
    }
}
