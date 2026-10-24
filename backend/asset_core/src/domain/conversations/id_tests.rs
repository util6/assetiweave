use super::*;

#[test]
fn test_derive_part_id_consistency() {
    let turn_id = "turn-12345";
    let part_id_0 = derive_conversation_part_id(turn_id, 0);
    let part_id_1 = derive_conversation_part_id(turn_id, 1);
    let web_id_0 = derive_web_record_part_id(turn_id, 0);

    assert!(part_id_0.starts_with("conversation-part-"));
    assert!(part_id_1.starts_with("conversation-part-"));
    assert!(web_id_0.starts_with("web-record-part-"));
    assert_ne!(part_id_0, part_id_1);
    assert_ne!(part_id_0, web_id_0);

    // Repeated calls produce identical IDs
    assert_eq!(part_id_0, derive_conversation_part_id(turn_id, 0));
}

#[test]
fn test_derive_part_id_key() {
    let turn_id = "turn-abcdef";
    let part_id = derive_conversation_part_id(turn_id, 0);
    let key = derive_part_id_key(&part_id).expect("derive key from standard part id");

    let hex_part = part_id.strip_prefix("conversation-part-").unwrap();
    let expected_hex = &hex_part[..16];
    let key_hex = key.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(key_hex, expected_hex);
}

#[test]
fn test_parse_id_key_prefix_exact() {
    let full_id =
        "conversation-part-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let parsed = parse_id_key_prefix(full_id).expect("parse full id");
    assert_eq!(
        parsed.lookup,
        IdKeyLookup::Exact([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef])
    );
    assert_eq!(parsed.node_order, None);

    let with_node = format!("{full_id}-node-3");
    let parsed_node = parse_id_key_prefix(&with_node).expect("parse with node");
    assert_eq!(
        parsed_node.lookup,
        IdKeyLookup::Exact([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef])
    );
    assert_eq!(parsed_node.node_order, Some(3));
}

#[test]
fn test_parse_id_key_prefix_even_range() {
    // 8 hex chars -> 4 bytes
    let short_id = "01234567";
    let parsed = parse_id_key_prefix(short_id).expect("parse 8-hex short id");
    match parsed.lookup {
        IdKeyLookup::Range { low, high } => {
            assert_eq!(low, [0x01, 0x23, 0x45, 0x67, 0, 0, 0, 0]);
            assert_eq!(high, Some([0x01, 0x23, 0x45, 0x68, 0, 0, 0, 0]));
        }
        _ => panic!("expected Range"),
    }

    // Overflow check: ff
    let parsed_ff = parse_id_key_prefix("ff").expect("parse ff");
    match parsed_ff.lookup {
        IdKeyLookup::Range { low, high } => {
            assert_eq!(low, [0xff, 0, 0, 0, 0, 0, 0, 0]);
            assert_eq!(high, None);
        }
        _ => panic!("expected Range with None high"),
    }
}

#[test]
fn test_parse_id_key_prefix_odd_range() {
    // 7 hex chars: 012345a
    let odd_id = "012345a";
    let parsed = parse_id_key_prefix(odd_id).expect("parse 7-hex odd id");
    match parsed.lookup {
        IdKeyLookup::Range { low, high } => {
            assert_eq!(low, [0x01, 0x23, 0x45, 0xa0, 0, 0, 0, 0]);
            assert_eq!(high, Some([0x01, 0x23, 0x45, 0xb0, 0, 0, 0, 0]));
        }
        _ => panic!("expected Range"),
    }

    // Nibble 15 (f) with carry: 012345f
    let carry_id = "012345f";
    let parsed_carry = parse_id_key_prefix(carry_id).expect("parse nibble carry");
    match parsed_carry.lookup {
        IdKeyLookup::Range { low, high } => {
            assert_eq!(low, [0x01, 0x23, 0x45, 0xf0, 0, 0, 0, 0]);
            assert_eq!(high, Some([0x01, 0x23, 0x46, 0x00, 0, 0, 0, 0]));
        }
        _ => panic!("expected Range with carry"),
    }
}
