/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::marker::PhantomData;

use crate::{
    builder::{CacheStoreBuilder, NodeStoreBuilder, ValueStoreBuilder},
    elem_type::{
        AccessMode, AddressKind, BitMask, CachingMode, DisplayNotation, Endianness,
        FloatRepresentation, ImmOrPNode, IntegerRepresentation, MergePriority, NameSpace,
        NamedValue, PIndex, PValue, RegPIndex, Sign, Slope, StandardNameSpace, ValueIndexed,
        ValueKind, Visibility,
    },
    interface::INode,
    store::{FloatId, IntegerId, NodeData, NodeId},
    IntSwissKnifeNode,
};

use super::{
    elem_name::{
        ADDRESS, BIT, INDEX, INT_SWISS_KNIFE, NAME, OFFSET, P_ADDRESS, P_INDEX, P_OFFSET, P_VALUE,
        P_VALUE_COPY, P_VALUE_INDEXED, VALUE, VALUE_INDEXED,
    },
    xml, Parse,
};

macro_rules! match_text_view{
    ($text:expr,
        $s1:expr => $var1:expr,
        $($s:expr => $var:expr,)*
    ) => {
        if $text == $s1 {
            $var1
        } $(else if $text == $s {
            $var
        })* else {
            unreachable!()
        }
    }
}

impl Default for NameSpace {
    fn default() -> Self {
        Self::Custom
    }
}

impl From<&str> for NameSpace {
    fn from(value: &str) -> Self {
        match value {
            "Standard" => Self::Standard,
            "Custom" => Self::Custom,
            _ => unreachable!(),
        }
    }
}

impl Parse for NameSpace {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        match_text_view!(text,
            "Standard" => Self::Standard,
            "Custom" => Self::Custom,
        )
    }
}

impl Default for Visibility {
    fn default() -> Self {
        Self::Beginner
    }
}

impl Parse for Visibility {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        match_text_view!(text,
            "Beginner" => Self::Beginner,
            "Expert" => Self::Expert,
            "Guru" => Self::Guru,
            "Invisible" => Self::Invisible,
        )
    }
}

impl From<&str> for MergePriority {
    fn from(value: &str) -> Self {
        match value {
            "1" => Self::High,
            "0" => Self::Mid,
            "-1" => Self::Low,
            _ => unreachable!(),
        }
    }
}

impl Default for MergePriority {
    fn default() -> Self {
        Self::Mid
    }
}

impl Parse for MergePriority {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        match_text_view!(text,
            "1" => Self::High,
            "0" => Self::Mid,
            "-1" => Self::Low,
        )
    }
}

impl Parse for AccessMode {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        match_text_view!(text,
            "RO" => Self::RO,
            "WO" => Self::WO,
            "RW" => Self::RW,
        )
    }
}

impl Parse for ImmOrPNode<i64> {
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        let peeked_text = node.peek().unwrap().text();
        if peeked_text.view().chars().next().unwrap().is_alphabetic() {
            Self::PNode(node.parse(node_builder, value_builder, cache_builder))
        } else {
            Self::Imm(node.parse(node_builder, value_builder, cache_builder))
        }
    }
}

impl Parse for ImmOrPNode<f64> {
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        let peeked_text = node.peek().unwrap().text();

        if peeked_text == "INF"
            || peeked_text == "-INF"
            || peeked_text == "NaN"
            || !peeked_text.view().chars().next().unwrap().is_alphabetic()
        {
            Self::Imm(node.parse(node_builder, value_builder, cache_builder))
        } else {
            Self::PNode(node.parse(node_builder, value_builder, cache_builder))
        }
    }
}

impl Parse for ImmOrPNode<bool> {
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        if convert_to_bool_opt(&node.peek().unwrap().text().view()).is_some() {
            Self::Imm(node.parse(node_builder, value_builder, cache_builder))
        } else {
            Self::PNode(node.parse(node_builder, value_builder, cache_builder))
        }
    }
}

macro_rules! impl_parse_for_imm_or_pnode_id {
    ($id:ty, $value_ty:ty) => {
        impl Parse for ImmOrPNode<$id> {
            fn parse(
                node: &mut xml::Node,
                node_builder: &mut impl NodeStoreBuilder,
                value_builder: &mut impl ValueStoreBuilder,
                cache_builder: &mut impl CacheStoreBuilder,
            ) -> Self {
                let node: ImmOrPNode<$value_ty> =
                    node.parse(node_builder, value_builder, cache_builder);
                match node {
                    ImmOrPNode::Imm(i) => {
                        let id = value_builder.store(i);
                        ImmOrPNode::Imm(id)
                    }
                    ImmOrPNode::PNode(id) => ImmOrPNode::PNode(id),
                }
            }
        }
    };
}

impl_parse_for_imm_or_pnode_id!(IntegerId, i64);
impl_parse_for_imm_or_pnode_id!(FloatId, f64);

impl Default for IntegerRepresentation {
    fn default() -> Self {
        Self::PureNumber
    }
}

impl Parse for IntegerRepresentation {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        use IntegerRepresentation::{
            Boolean, HexNumber, IpV4Address, Linear, Logarithmic, MacAddress, PureNumber,
        };

        let value = node.next_text().unwrap();
        match_text_view!(value,
            "Linear" => Linear,
            "Logarithmic" => Logarithmic,
            "Boolean" => Boolean,
            "PureNumber" => PureNumber,
            "HexNumber" => HexNumber,
            "IPV4Address" => IpV4Address,
            "MACAddress" => MacAddress,
        )
    }
}

impl Parse for FloatRepresentation {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        match_text_view! {text,
            "Linear" => Self::Linear,
            "Logarithmic" => Self::Logarithmic,
            "PureNumber" => Self::PureNumber,
        }
    }
}

impl Default for FloatRepresentation {
    fn default() -> Self {
        Self::PureNumber
    }
}

impl Parse for Slope {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        match_text_view! {text,
            "Increasing" => Self::Increasing,
            "Decreasing" => Self::Decreasing,
            "Varying" => Self::Varying,
            "Automatic" => Self::Automatic,
        }
    }
}

impl Default for Slope {
    fn default() -> Self {
        Self::Automatic
    }
}

impl Default for DisplayNotation {
    fn default() -> Self {
        Self::Automatic
    }
}

impl Parse for DisplayNotation {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        match_text_view! {text,
            "Automatic" => Self::Automatic,
            "Fixed" => Self::Fixed,
            "Scientific" => Self::Scientific,
        }
    }
}

impl From<&str> for StandardNameSpace {
    fn from(value: &str) -> Self {
        match value {
            "None" => Self::None,
            "IIDC" => Self::IIDC,
            "GEV" => Self::GEV,
            "CL" => Self::CL,
            "USB" => Self::USB,
            _ => unreachable!(),
        }
    }
}

impl Default for CachingMode {
    fn default() -> Self {
        Self::WriteThrough
    }
}

impl Parse for CachingMode {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        match_text_view! {text,
            "WriteThrough" => Self::WriteThrough,
            "WriteAround" => Self::WriteAround,
            "NoCache" => Self::NoCache,
        }
    }
}

impl<T> Parse for NamedValue<T>
where
    T: Clone + PartialEq + Parse,
{
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        let name = node.peek().unwrap().attribute_of(NAME).unwrap().into();
        let value = node.parse(node_builder, value_builder, cache_builder);
        Self { name, value }
    }
}

pub(super) fn convert_to_bool(value: &str) -> bool {
    match convert_to_bool_opt(value) {
        Some(b) => b,
        _ => unreachable!(),
    }
}

pub(super) fn convert_to_bool_opt(value: &str) -> Option<bool> {
    match value {
        "Yes" | "true" => Some(true),
        "No" | "false" => Some(false),
        _ => None,
    }
}

impl Parse for bool {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        convert_to_bool(&text.view())
    }
}

pub(super) fn convert_to_int(value: &str) -> i64 {
    if value.starts_with("0x") || value.starts_with("0X") {
        i64::from_str_radix(&value[2..], 16).unwrap()
    } else {
        value.parse().unwrap()
    }
}

pub(super) fn convert_to_uint(value: &str) -> u64 {
    if value.starts_with("0x") || value.starts_with("0X") {
        u64::from_str_radix(&value[2..], 16).unwrap()
    } else {
        value.parse().unwrap()
    }
}

impl Parse for i64 {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let value = node.next_text().unwrap();
        convert_to_int(&value.view())
    }
}

impl Parse for u64 {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let value = node.next_text().unwrap();
        convert_to_uint(&value.view())
    }
}

impl Parse for f64 {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let value = node.next_text().unwrap().view();
        if value == "INF" {
            f64::INFINITY
        } else if value == "-INF" {
            f64::NEG_INFINITY
        } else {
            value.parse().unwrap()
        }
    }
}

impl Parse for String {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        node.next_text().unwrap().view().into()
    }
}

impl Parse for NodeId {
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        node_builder.get_or_intern(text.view())
    }
}

macro_rules! impl_parse_for_value_id {
    ($id:ty, $value_ty:ty) => {
        impl Parse for $id {
            fn parse(
                node: &mut xml::Node,
                node_builder: &mut impl NodeStoreBuilder,
                value_builder: &mut impl ValueStoreBuilder,
                cache_builder: &mut impl CacheStoreBuilder,
            ) -> Self {
                let value: $value_ty = node.parse(node_builder, value_builder, cache_builder);
                let id = value_builder.store(value);
                id
            }
        }
    };
}

impl_parse_for_value_id!(IntegerId, i64);
impl_parse_for_value_id!(FloatId, f64);

impl<T> Parse for ValueKind<T>
where
    T: Clone + Parse + PartialEq,
    ImmOrPNode<T>: Parse,
{
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        let peek = node.peek().unwrap();
        match peek.tag_name() {
            VALUE => ValueKind::Value(node.parse(node_builder, value_builder, cache_builder)),
            P_VALUE_COPY | P_VALUE => {
                let p_value = node.parse(node_builder, value_builder, cache_builder);
                ValueKind::PValue(p_value)
            }
            P_INDEX => {
                let p_index = node.parse(node_builder, value_builder, cache_builder);
                ValueKind::PIndex(p_index)
            }
            _ => unreachable!(),
        }
    }
}

impl<T> Parse for PValue<T> {
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        // NOTE: The pValue can be sandwiched between two pValueCopy sequence.
        let mut p_value_copies =
            node.parse_while(P_VALUE_COPY, node_builder, value_builder, cache_builder);

        let p_value = node.parse(node_builder, value_builder, cache_builder);

        let node_ids: Vec<NodeId> =
            node.parse_while(P_VALUE_COPY, node_builder, value_builder, cache_builder);
        p_value_copies.extend(node_ids);

        Self {
            p_value,
            p_value_copies,
            phantom: PhantomData,
        }
    }
}

impl<T> Parse for PIndex<T>
where
    T: Clone + PartialEq + Parse,
    ImmOrPNode<T>: Parse,
{
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        let p_index = node.parse(node_builder, value_builder, cache_builder);

        let mut value_indexed = vec![];
        while let Some(indexed) = node
            .parse_if(VALUE_INDEXED, node_builder, value_builder, cache_builder)
            .or_else(|| node.parse_if(P_VALUE_INDEXED, node_builder, value_builder, cache_builder))
        {
            value_indexed.push(indexed);
        }

        let value_default = node.parse(node_builder, value_builder, cache_builder);

        Self {
            p_index,
            value_indexed,
            value_default,
        }
    }
}

impl<T> Parse for ValueIndexed<T>
where
    T: Clone + PartialEq + Parse,
    ImmOrPNode<T>: Parse,
{
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        let index = convert_to_int(node.peek().unwrap().attribute_of(INDEX).unwrap());
        let indexed = node.parse(node_builder, value_builder, cache_builder);
        Self { index, indexed }
    }
}

impl Parse for AddressKind {
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        let peeked_node = node.peek().unwrap();
        match peeked_node.tag_name() {
            ADDRESS | P_ADDRESS => {
                Self::Address(node.parse(node_builder, value_builder, cache_builder))
            }
            INT_SWISS_KNIFE => {
                let swiss_knife: IntSwissKnifeNode =
                    node.next()
                        .unwrap()
                        .parse(node_builder, value_builder, cache_builder);
                let id = swiss_knife.node_base().id();
                node_builder.store_node(id, NodeData::IntSwissKnife(swiss_knife.into()));
                Self::IntSwissKnife(id)
            }
            P_INDEX => Self::PIndex(node.parse(node_builder, value_builder, cache_builder)),
            _ => unreachable!(),
        }
    }
}

impl Parse for RegPIndex {
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        let next_node = node.peek().unwrap();

        let imm_offset = next_node
            .attribute_of(OFFSET)
            .map(|s| ImmOrPNode::Imm(convert_to_int(s)));
        let pnode_offset = next_node
            .attribute_of(P_OFFSET)
            .map(|s| ImmOrPNode::PNode(node_builder.get_or_intern(s)));
        let offset = imm_offset.xor(pnode_offset);

        let p_index = node.parse(node_builder, value_builder, cache_builder);

        Self { offset, p_index }
    }
}

impl Default for Endianness {
    fn default() -> Self {
        Self::LE
    }
}

impl Parse for Endianness {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        match_text_view! {text,
            "LittleEndian" => Self::LE,
            "BigEndian" => Self::BE,
        }
    }
}

impl Default for Sign {
    fn default() -> Self {
        Self::Unsigned
    }
}

impl Parse for Sign {
    fn parse(
        node: &mut xml::Node,
        _: &mut impl NodeStoreBuilder,
        _: &mut impl ValueStoreBuilder,
        _: &mut impl CacheStoreBuilder,
    ) -> Self {
        let text = node.next_text().unwrap();
        match_text_view! {text,
            "Signed" => Self::Signed,
            "Unsigned" => Self::Unsigned,
        }
    }
}

impl Parse for BitMask {
    fn parse(
        node: &mut xml::Node,
        node_builder: &mut impl NodeStoreBuilder,
        value_builder: &mut impl ValueStoreBuilder,
        cache_builder: &mut impl CacheStoreBuilder,
    ) -> Self {
        node.parse_if(BIT, node_builder, value_builder, cache_builder)
            .map_or_else(
                || {
                    let lsb = node.parse(node_builder, value_builder, cache_builder);
                    let msb = node.parse(node_builder, value_builder, cache_builder);
                    Self::Range { lsb, msb }
                },
                Self::SingleBit,
            )
    }
}
