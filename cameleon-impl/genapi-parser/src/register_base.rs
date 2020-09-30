use super::{elem_type::*, node_base::*, xml, Parse};

#[derive(Debug, Clone)]
pub struct RegisterBase {
    elem_base: NodeElementBase,

    streamable: bool,

    address_kinds: Vec<register_node_elem::AddressKind>,

    length: ImmOrPNode<i64>,

    access_mode: AccessMode,

    p_port: String,

    cacheable: CachingMode,

    polling_time: Option<i64>,

    p_invalidators: Vec<String>,
}

impl RegisterBase {
    pub fn streamable(&self) -> bool {
        self.streamable
    }

    pub fn address_kinds(&self) -> &[register_node_elem::AddressKind] {
        &self.address_kinds
    }

    pub fn length(&self) -> &ImmOrPNode<i64> {
        &self.length
    }

    pub fn access_mode(&self) -> AccessMode {
        self.access_mode
    }

    pub fn p_port(&self) -> &str {
        &self.p_port
    }

    pub fn cacheable(&self) -> CachingMode {
        self.cacheable
    }

    pub fn polling_time(&self) -> Option<i64> {
        self.polling_time
    }

    pub fn p_invalidators(&self) -> &[String] {
        &self.p_invalidators
    }

    pub(super) fn elem_base(&self) -> &NodeElementBase {
        &self.elem_base
    }
}

impl Parse for RegisterBase {
    fn parse(node: &mut xml::Node) -> Self {
        let elem_base = node.parse();

        let streamable = node.parse_if("Streamable").unwrap_or_default();

        let mut address_kinds = vec![];
        while let Some(addr_kind) = node
            .parse_if("Address")
            .or_else(|| node.parse_if("IntSwissKnife"))
            .or_else(|| node.parse_if("pAddress"))
            .or_else(|| node.parse_if("pIndex"))
        {
            address_kinds.push(addr_kind);
        }

        let length = node.parse();

        let access_mode = node.parse_if("AccessMode").unwrap_or(AccessMode::RO);

        let p_port = node.parse();

        let cacheable = node.parse_if("Cachable").unwrap_or_default();

        let polling_time = node.parse_if("PollingTime");

        let mut p_invalidators = vec![];
        while let Some(invalidator) = node.parse_if("pInvalidator") {
            p_invalidators.push(invalidator);
        }

        Self {
            elem_base,
            streamable,
            address_kinds,
            length,
            access_mode,
            p_port,
            cacheable,
            polling_time,
            p_invalidators,
        }
    }
}
