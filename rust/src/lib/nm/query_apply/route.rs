// SPDX-License-Identifier: Apache-2.0

use super::super::nm_dbus::{NmConnection, NmIpRoute, NmSettingIp};

const NM_IP_SETTING_ROUTE_TABLE_DEFAULT: u32 = 0;
const NM_IP_SETTING_ROUTE_METRIC_DEFAULT: i64 = -1;
const IPV6_METRIC_COHERCED_DEFAULT: u32 = 1024; // Coherced by kernel 0->1024

pub(crate) fn is_route_removed(
    new_nm_conn: &NmConnection,
    cur_nm_conn: &NmConnection,
) -> bool {
    nm_setting_is_route_removed(
        new_nm_conn.ipv4.as_ref(),
        cur_nm_conn.ipv4.as_ref(),
        false,
    ) || nm_setting_is_route_removed(
        new_nm_conn.ipv6.as_ref(),
        cur_nm_conn.ipv6.as_ref(),
        true,
    )
}

fn nm_setting_is_route_removed(
    new_nm_sett: Option<&NmSettingIp>,
    cur_nm_sett: Option<&NmSettingIp>,
    is_ipv6: bool,
) -> bool {
    let new_routes = clone_normalized_routes(new_nm_sett, is_ipv6);
    let cur_routes = clone_normalized_routes(cur_nm_sett, is_ipv6);
    cur_routes
        .iter()
        .any(|cur_route| !new_routes.contains(cur_route))
}

fn clone_normalized_routes(
    ip_sett: Option<&NmSettingIp>,
    is_ipv6: bool,
) -> Vec<NmIpRoute> {
    // Routes defined by nmstate will always has table and metric set, so there
    // is no problem comparing them.
    // On routes defined in NM directly, they may depend on the route-metric and
    // route-table properties of the ipv4 and ipv6 settings. Use them to get the
    // actual values.
    // They may even fall back to a globally default value. In that case we can
    // not know what value is. Use None to fail the comparison so we can
    // properly install the new desired route, with table and metric defined.
    let default_table = ip_sett
        .and_then(|ip| ip.route_table)
        .filter(|tbl| *tbl != NM_IP_SETTING_ROUTE_TABLE_DEFAULT);
    let mut default_metric = ip_sett
        .and_then(|ip| ip.route_metric)
        .filter(|mtr| *mtr != NM_IP_SETTING_ROUTE_METRIC_DEFAULT)
        .map(|mtr| mtr as u32);
    if is_ipv6 && default_metric == Some(0) {
        default_metric = Some(IPV6_METRIC_COHERCED_DEFAULT);
    }

    let routes = ip_sett.map(|ip| ip.routes.as_slice()).unwrap_or(&[]);
    routes
        .iter()
        .map(|rt| {
            let mut new_rt = rt.clone();
            new_rt.table = rt.table.or(default_table);
            new_rt.metric = rt.metric.or(default_metric);
            new_rt
        })
        .collect()
}
