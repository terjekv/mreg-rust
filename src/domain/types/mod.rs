mod addresses;
mod dns_names;
mod encoded;
mod identifiers;
mod numerics;
mod record_values;
mod update_field;

pub use addresses::*;
pub use dns_names::*;
pub use encoded::*;
pub use identifiers::*;
pub use numerics::*;
pub use record_values::record_type_names;
pub use record_values::*;
pub use update_field::*;

#[cfg(test)]
mod tests {
    use super::{
        BacnetIdentifier, CidrValue, CommunityName, DnsCharacterString, DnsName, DomainNameValue,
        EmailAddressValue, HexEncodedValue, HostGroupName, Hostname, IpAddressValue, Ipv4AddrValue,
        Ipv6AddrValue, LabelName, NetworkPolicyName, RecordTypeName, SerialNumber, SoaSeconds, Ttl,
        VlanId,
    };

    #[test]
    fn dns_names_are_canonicalized_to_lowercase_without_trailing_dot() {
        let value = DnsName::new("Ns1.Example.ORG.").expect("dns name should parse");
        assert_eq!(value.as_str(), "ns1.example.org");
    }

    #[test]
    fn domain_name_value_allows_root() {
        let value = DomainNameValue::new(".").expect("root should parse");
        assert!(value.is_root());
        assert_eq!(value.as_str(), ".");
    }

    #[test]
    fn hostname_rejects_underscores() {
        assert!(Hostname::new("bad_name.example.org").is_err());
    }

    #[test]
    fn cidr_parses_successfully() {
        let value = CidrValue::new("10.0.0.0/24").expect("cidr should parse");
        assert_eq!(value.as_str(), "10.0.0.0/24");
    }

    #[test]
    fn label_name_normalizes_to_lowercase() {
        let value = LabelName::new("Production_Label").expect("label name should parse");
        assert_eq!(value.as_str(), "production_label");
    }

    #[test]
    fn email_address_validates() {
        let value = EmailAddressValue::new("Admin@Example.Org").expect("email should parse");
        assert_eq!(value.as_str(), "admin@example.org");
    }

    #[test]
    fn dns_character_string_rejects_long_values() {
        let oversized = "a".repeat(256);
        assert!(DnsCharacterString::new(oversized).is_err());
    }

    #[test]
    fn hex_value_normalizes_to_lowercase() {
        let value = HexEncodedValue::new("ABCD1234").expect("hex should parse");
        assert_eq!(value.as_str(), "abcd1234");
    }

    #[test]
    fn ttl_rejects_out_of_range_values() {
        assert!(Ttl::new(i32::MAX as u32 + 1).is_err());
    }

    #[test]
    fn policy_name_normalizes_to_lowercase() {
        let value = NetworkPolicyName::new("Campus-Core").expect("policy name should parse");
        assert_eq!(value.as_str(), "campus-core");
    }

    #[test]
    fn host_group_name_normalizes_to_lowercase() {
        let value = HostGroupName::new("Server_Farm").expect("group name should parse");
        assert_eq!(value.as_str(), "server_farm");
    }

    #[test]
    fn community_name_normalizes_to_lowercase() {
        let value = CommunityName::new("Prod.Network").expect("community should parse");
        assert_eq!(value.as_str(), "prod.network");
    }

    #[test]
    fn bacnet_identifier_rejects_zero() {
        assert!(BacnetIdentifier::new(0).is_err());
    }

    #[test]
    fn bacnet_identifier_rejects_values_above_i32_max() {
        assert!(BacnetIdentifier::new(i32::MAX as u32 + 1).is_err());
    }

    #[test]
    fn serial_next_rfc1912_increments_within_day() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 30).unwrap();
        let serial = SerialNumber::new(202603300000).expect("serial");
        let next = serial.next_rfc1912(today).expect("next serial");
        assert_eq!(next.as_u64(), 202603300001);
    }

    #[test]
    fn serial_next_rfc1912_rolls_to_new_day() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();
        let serial = SerialNumber::new(202603300005).expect("serial");
        let next = serial.next_rfc1912(today).expect("next serial");
        assert_eq!(next.as_u64(), 202603310000);
    }

    #[test]
    fn serial_next_rfc1912_handles_clock_skew() {
        // Serial is ahead of today (clock went backwards)
        let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 28).unwrap();
        let serial = SerialNumber::new(202603300005).expect("serial");
        let next = serial.next_rfc1912(today).expect("next serial");
        assert_eq!(next.as_u64(), 202603300006);
    }

    #[test]
    fn serial_next_rfc1912_from_legacy_value() {
        // Starting from a low serial (legacy non-date-based)
        let today = chrono::NaiveDate::from_ymd_opt(2026, 3, 30).unwrap();
        let serial = SerialNumber::new(1).expect("serial");
        let next = serial.next_rfc1912(today).expect("next serial");
        assert_eq!(next.as_u64(), 202603300000);
    }

    #[test]
    fn dns_name_rejects_empty() {
        assert!(DnsName::new("").is_err());
    }

    #[test]
    fn dns_name_rejects_root_only() {
        assert!(DnsName::new(".").is_err());
    }

    #[test]
    fn dns_name_strips_trailing_dot() {
        let value = DnsName::new("example.org.").expect("should parse");
        assert_eq!(value.as_str(), "example.org");
    }

    #[test]
    fn dns_name_rejects_double_dot() {
        assert!(DnsName::new("example..org").is_err());
    }

    #[test]
    fn hostname_rejects_wildcards() {
        assert!(Hostname::new("*.example.org").is_err());
    }

    #[test]
    fn hostname_rejects_leading_hyphen() {
        assert!(Hostname::new("-bad.example.org").is_err());
    }

    #[test]
    fn hostname_rejects_trailing_hyphen() {
        assert!(Hostname::new("bad-.example.org").is_err());
    }

    #[test]
    fn ipv4_addr_rejects_invalid() {
        assert!(Ipv4AddrValue::new("not-an-ip").is_err());
    }

    #[test]
    fn ipv6_addr_rejects_invalid() {
        assert!(Ipv6AddrValue::new("not-an-ip").is_err());
    }

    #[test]
    fn ip_address_accepts_v4() {
        let value = IpAddressValue::new("10.0.0.1").expect("should parse");
        assert_eq!(value.as_str(), "10.0.0.1");
    }

    #[test]
    fn ip_address_accepts_v6() {
        let value = IpAddressValue::new("::1").expect("should parse");
        assert_eq!(value.as_str(), "::1");
    }

    #[test]
    fn cidr_rejects_invalid() {
        assert!(CidrValue::new("not-a-cidr").is_err());
    }

    #[test]
    fn hex_value_rejects_odd_length() {
        assert!(HexEncodedValue::new("abc").is_err());
    }

    #[test]
    fn hex_value_rejects_non_hex() {
        assert!(HexEncodedValue::new("zzzz").is_err());
    }

    #[test]
    fn ttl_accepts_zero() {
        let value = Ttl::new(0).expect("zero should be valid");
        assert_eq!(value.as_u32(), 0);
    }

    #[test]
    fn ttl_accepts_max() {
        let value = Ttl::new(i32::MAX as u32).expect("i32::MAX should be valid");
        assert_eq!(value.as_u32(), i32::MAX as u32);
    }

    #[test]
    fn serial_rejects_overflow() {
        assert!(SerialNumber::new(i64::MAX as u64 + 1).is_err());
    }

    #[test]
    fn record_type_name_normalizes_to_uppercase() {
        let value = RecordTypeName::new("cname").expect("should parse");
        assert_eq!(value.as_str(), "CNAME");
    }

    #[test]
    fn soa_seconds_accepts_valid_value() {
        let value = SoaSeconds::new(10_800).expect("should be valid");
        assert_eq!(value.as_u32(), 10_800);
        assert_eq!(value.as_i32(), 10_800);
    }

    #[test]
    fn soa_seconds_rejects_out_of_range() {
        assert!(SoaSeconds::new(i32::MAX as u32 + 1).is_err());
    }

    #[test]
    fn soa_seconds_accepts_zero() {
        let value = SoaSeconds::new(0).expect("zero should be valid");
        assert_eq!(value.as_u32(), 0);
    }

    #[test]
    fn vlan_id_accepts_valid_values() {
        let value = VlanId::new(0).expect("0 should be valid");
        assert_eq!(value.as_u32(), 0);
        let value = VlanId::new(4094).expect("4094 should be valid");
        assert_eq!(value.as_u32(), 4094);
    }

    #[test]
    fn vlan_id_rejects_out_of_range() {
        assert!(VlanId::new(4095).is_err());
        assert!(VlanId::new(5000).is_err());
    }
}
