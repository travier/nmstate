# SPDX-License-Identifier: Apache-2.0

import pytest
import libnmstate
from libnmstate.schema import Interface

from .testlib.assertlib import assert_state_match
from .testlib.ifacelib import get_mac_address
from .testlib.statelib import show_only
from .testlib.yaml import load_yaml


@pytest.fixture
def clean_up():
    yield
    libnmstate.apply(
        load_yaml(
            """---
            interfaces:
            - name: dummy1
              type: dummy
              state: absent
            - name: dummy2
              type: dummy
              state: absent
            - name: port1
              type: ethernet
              state: absent
            - name: port2
              type: ethernet
              state: absent
            - name: bond0
              type: bond
              state: absent
            - name: br0
              type: linux-bridge
              state: absent
            - name: ovs-br0
              type: ovs-bridge
              state: absent
            - name: vrf0
              type: vrf
              state: absent
          """
        )
    )


def test_bond_port_ref_by_mac(eth1_up, eth2_up, clean_up):
    port1_mac = get_mac_address("eth1")
    port2_mac = get_mac_address("eth2")

    state = load_yaml(
        """---
        interfaces:
        - name: port1
          type: ethernet
          identifier: mac-address
          mac-address: {}
        - name: port2
          type: ethernet
          identifier: mac-address
          mac-address: {}
        - name: bond0
          type: bond
          state: up
          link-aggregation:
            mode: balance-rr
            port:
            - port1
            - port2""".format(
            port1_mac, port2_mac
        )
    )

    libnmstate.apply(state)

    expected_state = load_yaml(
        """---
        interfaces:
        - name: bond0
          type: bond
          state: up
          link-aggregation:
            mode: balance-rr
            port:
            - eth1
            - eth2"""
    )

    assert_state_match(expected_state)


def test_bond_port_conf_ref_by_mac(eth1_up, eth2_up, clean_up):
    port1_mac = get_mac_address("eth1")
    port2_mac = get_mac_address("eth2")

    state = load_yaml(
        """---
        interfaces:
        - name: port1
          type: ethernet
          identifier: mac-address
          mac-address: {}
        - name: port2
          type: ethernet
          identifier: mac-address
          mac-address: {}
        - name: bond0
          type: bond
          state: up
          link-aggregation:
            mode: balance-rr
            ports-config:
            - name: port1
            - name: port2""".format(
            port1_mac, port2_mac
        )
    )

    libnmstate.apply(state)

    expected_state = load_yaml(
        """---
        interfaces:
        - name: bond0
          type: bond
          state: up
          link-aggregation:
            mode: balance-rr
            port:
            - eth1
            - eth2"""
    )

    assert_state_match(expected_state)


def test_linux_bridge_port_ref_by_mac(eth1_up, eth2_up, clean_up):
    port1_mac = get_mac_address("eth1")
    port2_mac = get_mac_address("eth2")

    state = load_yaml(
        """---
        interfaces:
        - name: port1
          type: ethernet
          identifier: mac-address
          mac-address: {}
        - name: port2
          type: ethernet
          identifier: mac-address
          mac-address: {}
        - name: br0
          type: linux-bridge
          state: up
          bridge:
            port:
            - name: port1
            - name: port2""".format(
            port1_mac, port2_mac
        )
    )

    libnmstate.apply(state)

    expected_state = load_yaml(
        """---
        interfaces:
        - name: br0
          type: linux-bridge
          state: up
          bridge:
            port:
            - name: eth1
            - name: eth2"""
    )

    assert_state_match(expected_state)


def test_ovs_bridge_port_ref_by_mac(eth1_up, eth2_up, clean_up):
    port1_mac = get_mac_address("eth1")
    port2_mac = get_mac_address("eth2")

    state = load_yaml(
        """---
        interfaces:
        - name: port1
          type: ethernet
          identifier: mac-address
          mac-address: {}
        - name: port2
          type: ethernet
          identifier: mac-address
          mac-address: {}
        - name: ovs-br0
          type: ovs-bridge
          state: up
          bridge:
            port:
            - name: port1
            - name: port2""".format(
            port1_mac, port2_mac
        )
    )

    libnmstate.apply(state)

    expected_state = load_yaml(
        """---
        interfaces:
        - name: ovs-br0
          type: ovs-bridge
          state: up
          bridge:
            port:
            - name: eth1
            - name: eth2"""
    )

    assert_state_match(expected_state)


def test_vrf_port_ref_by_mac(eth1_up, eth2_up, clean_up):
    port1_mac = get_mac_address("eth1")
    port2_mac = get_mac_address("eth2")

    state = load_yaml(
        """---
        interfaces:
        - name: port1
          type: ethernet
          identifier: mac-address
          mac-address: {}
        - name: port2
          type: ethernet
          identifier: mac-address
          mac-address: {}
        - name: vrf0
          type: vrf
          state: up
          vrf:
            route-table-id: 100
            port:
            - port1
            - port2""".format(
            port1_mac, port2_mac
        )
    )

    libnmstate.apply(state)

    expected_state = load_yaml(
        """---
        interfaces:
        - name: vrf0
          type: vrf
          state: up
          vrf:
            route-table-id: 100
            port:
            - eth1
            - eth2"""
    )

    assert_state_match(expected_state)


def test_port_ref_prefer_kernel_name(clean_up):
    state = load_yaml(
        """---
        interfaces:
        - name: dummy1
          type: dummy
          state: up
          mac-address: 32:BB:72:65:19:2A
        - name: dummy2
          profile-name: dummy1
          type: dummy
          state: up
          mac-address: 32:BB:72:65:19:2B
        - name: br0
          type: linux-bridge
          state: up
          bridge:
            ports:
            - name: dummy1"""
    )

    libnmstate.apply(state)
    dummy1_iface_state = show_only(("dummy1",))[Interface.KEY][0]
    dummy2_iface_state = show_only(("dummy2",))[Interface.KEY][0]

    assert dummy1_iface_state[Interface.CONTROLLER] == "br0"
    assert dummy1_iface_state[Interface.MAC] == "32:BB:72:65:19:2A"
    assert Interface.CONTROLLER not in dummy2_iface_state


def test_referring_to_duplicate_profile_names(eth1_up, eth2_up):
    port1_mac = get_mac_address("eth1")
    port2_mac = get_mac_address("eth2")

    state = load_yaml(
        """---
        interfaces:
        - name: br0
          type: linux-bridge
          state: up
          bridge:
            ports:
            - name: port1
        - name: eth1
          profile-name: port1
          type: dummy
          state: up
          identifier: mac-address
          mac-address: {}
        - name: eth2
          profile-name: port1
          type: dummy
          state: up
          identifier: mac-address
          mac-address: {}""".format(
            port1_mac, port2_mac
        )
    )

    with pytest.raises(libnmstate.error.NmstateValueError):
        libnmstate.apply(state)


def test_can_have_multiple_iface_holding_same_profile_name(eth1_up, eth2_up):
    eth1_mac = get_mac_address("eth1")
    eth2_mac = get_mac_address("eth2")

    state = load_yaml(
        """---
        interfaces:
        - name: eth1
          profile-name: port1
          type: ethernet
          state: up
          identifier: mac-address
          mac-address: {}
        - name: eth2
          profile-name: port1
          type: ethernet
          state: up
          identifier: mac-address
          mac-address: {}""".format(
            eth1_mac, eth2_mac
        )
    )

    libnmstate.apply(state)

    eth1_iface_state = show_only(("eth1",))[Interface.KEY][0]
    eth2_iface_state = show_only(("eth2",))[Interface.KEY][0]

    assert eth1_iface_state[Interface.MAC] == eth1_mac
    assert eth1_iface_state[Interface.PROFILE_NAME] == "port1"
    assert eth2_iface_state[Interface.MAC] == eth2_mac
    assert eth2_iface_state[Interface.PROFILE_NAME] == "port1"
