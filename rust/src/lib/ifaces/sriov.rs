use log::error;
use serde::{Deserialize, Serialize};

use crate::{ErrorKind, Interface, InterfaceType, Interfaces, NmstateError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SrIovConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_vfs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vfs: Option<Vec<SrIovVfConfig>>,
}

impl SrIovConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn update(&mut self, other: Option<&SrIovConfig>) {
        if let Some(other) = other {
            if let Some(total_vfs) = other.total_vfs {
                self.total_vfs = Some(total_vfs);
            }
            if let Some(vfs) = other.vfs.as_ref() {
                self.vfs = Some(vfs.clone());
            }
        }
    }

    // Convert VF MAC address to upper case
    pub(crate) fn pre_verify_cleanup(&mut self) {
        if let Some(vfs) = self.vfs.as_mut() {
            for vf in vfs {
                if let Some(address) = vf.mac_address.as_mut() {
                    address.make_ascii_uppercase()
                }
            }
        }
    }

    pub(crate) fn sriov_is_enabled(&self) -> bool {
        matches!(self.total_vfs, Some(i) if i > 0)
    }

    // Many SRIOV card require extra time for kernel and udev to setup the
    // VF interface. This function will wait VF interface been found in
    // cur_ifaces.
    // This function does not handle the decrease of SRIOV count(interface been
    // removed from kernel) as our test showed kernel does not require extra
    // time on deleting interface.
    pub(crate) fn verify_sriov(
        &self,
        pf_name: &str,
        cur_ifaces: &Interfaces,
    ) -> Result<(), NmstateError> {
        let cur_pf_iface =
            match cur_ifaces.get_iface(pf_name, InterfaceType::Ethernet) {
                Some(Interface::Ethernet(i)) => i,
                _ => {
                    let e = NmstateError::new(
                        ErrorKind::VerificationError,
                        format!("Failed to find PF interface {}", pf_name),
                    );
                    error!("{}", e);
                    return Err(e);
                }
            };

        let vfs = if let Some(vfs) = cur_pf_iface
            .ethernet
            .as_ref()
            .and_then(|eth_conf| eth_conf.sr_iov.as_ref())
            .and_then(|sriov_conf| sriov_conf.vfs.as_ref())
        {
            vfs
        } else {
            return Ok(());
        };
        for vf in vfs {
            if vf.iface_name.as_str().is_empty() {
                let e = NmstateError::new(
                    ErrorKind::VerificationError,
                    format!(
                        "Failed to find VF {} interface name of PF {}",
                        vf.id, pf_name
                    ),
                );
                error!("{}", e);
                return Err(e);
            } else if cur_ifaces
                .get_iface(vf.iface_name.as_str(), InterfaceType::Ethernet)
                .is_none()
            {
                let e = NmstateError::new(
                    ErrorKind::VerificationError,
                    format!(
                        "Find VF {} interface name {} of PF {} \
                        is not exist yet",
                        vf.id, &vf.iface_name, pf_name
                    ),
                );
                error!("{}", e);
                return Err(e);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SrIovVfConfig {
    pub id: u32,
    #[serde(skip)]
    pub(crate) iface_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spoof_check: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_tx_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tx_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vlan_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qos: Option<u32>,
}

impl SrIovVfConfig {
    pub fn new() -> Self {
        Self::default()
    }
}
