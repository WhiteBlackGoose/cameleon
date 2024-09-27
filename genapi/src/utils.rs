/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::{borrow::Cow, collections::HashMap, convert::TryInto};

use super::{
    elem_type::{Endianness, NamedValue, Sign},
    formula::EvaluationResult,
    formula::Expr,
    interface::{IBoolean, IEnumeration, IFloat, IInteger},
    store::{CacheStore, NodeId, NodeStore, ValueStore},
    Device, GenApiError, GenApiResult, ValueCtxt,
};

pub(super) fn bool_from_id<T: ValueStore, U: CacheStore>(
    node_id: NodeId,
    device: &mut impl Device,
    store: &impl NodeStore,
    cx: &mut ValueCtxt<T, U>,
) -> GenApiResult<bool> {
    if let Some(node) = node_id.as_iboolean_kind(store) {
        node.value(device, store, cx)
    } else if let Some(node) = node_id.as_iinteger_kind(store) {
        Ok(node.value(device, store, cx)? == 1)
    } else {
        Err(GenApiError::invalid_node(
            "the node doesn't implement `IInteger` nor `IBoolean".into(),
        ))
    }
}

pub(super) fn int_from_slice(
    slice: &[u8],
    endianness: Endianness,
    sign: Sign,
) -> GenApiResult<i64> {
    macro_rules! convert_from_slice {
        ($(($len:literal, $signed_ty:ty, $unsigned_ty:ty)),*) => {
            match (slice.len(), endianness, sign) {
                $(
                    ($len, Endianness::LE, Sign::Signed) => Ok(i64::from(<$signed_ty>::from_le_bytes(slice.try_into().unwrap()))),
                    ($len, Endianness::LE, Sign::Unsigned) => Ok(<$unsigned_ty>::from_le_bytes(slice.try_into().unwrap()) as i64),
                    ($len, Endianness::BE, Sign::Signed) => Ok(i64::from(<$signed_ty>::from_be_bytes(slice.try_into().unwrap()))),
                    ($len, Endianness::BE, Sign::Unsigned) => Ok(<$unsigned_ty>::from_be_bytes(slice.try_into().unwrap()) as i64),
                )*
                _ => Err(GenApiError::invalid_buffer("buffer length must be either 1/2/4/8 to convert to i64".into()))
            }
        }
    }

    convert_from_slice!((8, i64, u64), (4, i32, u32), (2, i16, u16), (1, i8, u8))
}

pub(super) fn bytes_from_int(
    value: i64,
    buf: &mut [u8],
    endianness: Endianness,
    sign: Sign,
) -> GenApiResult<()> {
    macro_rules! convert_to_slice {
        ($(($len:literal, $signed_ty:ty, $unsigned_ty:ty)),*) => {
            match (buf.len(), endianness, sign) {
                $(
                    ($len, Endianness::LE, Sign::Signed) => Ok(buf.copy_from_slice(&(value as $signed_ty).to_le_bytes())),
                    ($len, Endianness::LE, Sign::Unsigned) => Ok(buf.copy_from_slice(&(value as $unsigned_ty).to_le_bytes())),
                    ($len, Endianness::BE, Sign::Signed) => Ok(buf.copy_from_slice(&(value as $signed_ty).to_be_bytes())),
                    ($len, Endianness::BE, Sign::Unsigned) => Ok(buf.copy_from_slice(&(value as $unsigned_ty).to_be_bytes())),
                )*
                _ => Err(GenApiError::invalid_buffer("buffer length must be either 1/2/4/8 to convert to i64".into()))
            }
        }
    }

    convert_to_slice!((8, i64, u64), (4, i32, u32), (2, i16, u16), (1, i8, u8))
}

pub(super) fn float_from_slice(slice: &[u8], endianness: Endianness) -> GenApiResult<f64> {
    match (slice.len(), endianness) {
        (8, Endianness::LE) => Ok(f64::from_le_bytes(slice.try_into().unwrap())),
        (8, Endianness::BE) => Ok(f64::from_be_bytes(slice.try_into().unwrap())),
        (4, Endianness::LE) => Ok(f64::from(f32::from_le_bytes(slice.try_into().unwrap()))),
        (4, Endianness::BE) => Ok(f64::from(f32::from_be_bytes(slice.try_into().unwrap()))),
        _ => Err(GenApiError::invalid_buffer(
            "buffer length must be either 4/8 to convert to f64".into(),
        )),
    }
}

pub(super) fn bytes_from_float(
    value: f64,
    buf: &mut [u8],
    endianness: Endianness,
) -> GenApiResult<()> {
    match (buf.len(), endianness) {
        (8, Endianness::LE) => {
            buf.copy_from_slice(&value.to_le_bytes());
            Ok(())
        }
        (4, Endianness::LE) => {
            buf.copy_from_slice(&(value as f32).to_le_bytes());
            Ok(())
        }
        (8, Endianness::BE) => {
            buf.copy_from_slice(&value.to_be_bytes());
            Ok(())
        }
        (4, Endianness::BE) => {
            buf.copy_from_slice(&(value as f32).to_be_bytes());
            Ok(())
        }
        _ => Err(GenApiError::invalid_buffer(
            "buffer length must be either 4/8 to convert from f64".into(),
        )),
    }
}

pub(super) struct FormulaEnvCollector<'a, T> {
    p_variables: &'a [NamedValue<NodeId>],
    constants: &'a [NamedValue<T>],
    expressions: &'a [NamedValue<Expr>],
    var_env: HashMap<&'a str, Cow<'a, Expr>>,
}

impl<'a, T: Copy + Into<Expr>> FormulaEnvCollector<'a, T> {
    pub(super) fn new(
        p_variables: &'a [NamedValue<NodeId>],
        constants: &'a [NamedValue<T>],
        expressions: &'a [NamedValue<Expr>],
    ) -> Self {
        Self {
            p_variables,
            constants,
            expressions,
            var_env: HashMap::new(),
        }
    }
    pub(super) fn collect<U: ValueStore, S: CacheStore>(
        mut self,
        device: &mut impl Device,
        store: &impl NodeStore,
        cx: &mut ValueCtxt<U, S>,
    ) -> GenApiResult<HashMap<&'a str, Cow<'a, Expr>>> {
        // Collect variables.
        self.collect_variables(device, store, cx)?;

        // Collect constatns.
        for constant in self.constants {
            let name = constant.name();
            let value: Expr = (constant.value()).into();
            self.var_env.insert(name, Cow::Owned(value));
        }

        // Collect expressions.
        for expr in self.expressions {
            let name = expr.name();
            let value = expr.value_ref();
            self.var_env.insert(name, Cow::Borrowed(value));
        }

        Ok(self.var_env)
    }

    pub(super) fn insert<U: ValueStore, S: CacheStore>(
        &mut self,
        name: &'a str,
        nid: NodeId,
        device: &mut impl Device,
        store: &impl NodeStore,
        cx: &mut ValueCtxt<U, S>,
    ) -> GenApiResult<()> {
        let expr = expr_from_nid(nid, device, store, cx)?;
        self.insert_imm(name, expr);
        Ok(())
    }

    pub(super) fn insert_imm(&mut self, name: &'a str, imm: impl Into<Expr>) {
        self.var_env.insert(name, Cow::Owned(imm.into()));
    }

    pub(super) fn is_readable<U: ValueStore, S: CacheStore>(
        &self,
        device: &mut impl Device,
        store: &impl NodeStore,
        cx: &mut ValueCtxt<U, S>,
    ) -> GenApiResult<bool> {
        let mut res = true;
        for variable in self.p_variables {
            res &= is_nid_readable(variable.value(), device, store, cx)?;
        }
        Ok(res)
    }

    fn collect_variables<U: ValueStore, S: CacheStore>(
        &mut self,
        device: &mut impl Device,
        store: &impl NodeStore,
        cx: &mut ValueCtxt<U, S>,
    ) -> GenApiResult<()> {
        for variable in self.p_variables {
            let name = variable.name();
            let nid = variable.value();
            let expr = VariableKind::from_str(name)?.get_value(nid, device, store, cx)?;
            self.var_env.insert(name, Cow::Owned(expr));
        }
        Ok(())
    }
}

#[derive(Debug)]
enum VariableKind<'a> {
    Value,
    Min,
    Max,
    Inc,
    Enum(&'a str),
}

impl<'a> VariableKind<'a> {
    fn from_str(s: &'a str) -> GenApiResult<Self> {
        let split: Vec<&'a str> = s.splitn(3, '.').collect();
        Ok(match split.as_slice() {
            [_] | [_, "Value"] => Self::Value,
            [_, "Min"] => Self::Min,
            [_, "Max"] => Self::Max,
            [_, "Inc"] => Self::Inc,
            [_, "Enum", name] => Self::Enum(name),
            _ => {
                return Err(GenApiError::invalid_node(
                    format!("invalid `pVariable`: {}", s).into(),
                ))
            }
        })
    }

    fn get_value<T: ValueStore, U: CacheStore>(
        self,
        nid: NodeId,
        device: &mut impl Device,
        store: &impl NodeStore,
        cx: &mut ValueCtxt<T, U>,
    ) -> GenApiResult<Expr> {
        fn error(nid: NodeId, store: &impl NodeStore) -> GenApiError {
            GenApiError::invalid_node(format!("invalid `pVariable: {}`", nid.name(store)).into())
        }

        let expr: Expr = match self {
            Self::Value => expr_from_nid(nid, device, store, cx)?,
            Self::Min => {
                if let Some(node) = nid.as_iinteger_kind(store) {
                    node.min(device, store, cx)?.into()
                } else if let Some(node) = nid.as_ifloat_kind(store) {
                    node.min(device, store, cx)?.into()
                } else {
                    return Err(error(nid, store));
                }
            }
            Self::Max => {
                if let Some(node) = nid.as_iinteger_kind(store) {
                    node.max(device, store, cx)?.into()
                } else if let Some(node) = nid.as_ifloat_kind(store) {
                    node.max(device, store, cx)?.into()
                } else {
                    return Err(error(nid, store));
                }
            }
            Self::Inc => {
                if let Some(node) = nid.as_iinteger_kind(store) {
                    node.inc(device, store, cx)?
                        .ok_or_else(|| error(nid, store))?
                        .into()
                } else if let Some(node) = nid.as_ifloat_kind(store) {
                    node.inc(device, store, cx)?
                        .ok_or_else(|| error(nid, store))?
                        .into()
                } else {
                    return Err(error(nid, store));
                }
            }
            Self::Enum(name) => {
                if let Some(node) = nid.as_ienumeration_kind(store) {
                    node.entry_by_symbolic(name, store)
                        .ok_or_else(|| error(nid, store))
                        .map(|nid| nid.expect_enum_entry(store).unwrap())?
                        .value()
                        .into()
                } else {
                    return Err(error(nid, store));
                }
            }
        };

        Ok(expr)
    }
}

pub(super) fn is_nid_readable<T: ValueStore, U: CacheStore>(
    nid: NodeId,
    device: &mut impl Device,
    store: &impl NodeStore,
    cx: &mut ValueCtxt<T, U>,
) -> GenApiResult<bool> {
    Ok(if let Some(node) = nid.as_iinteger_kind(store) {
        node.is_readable(device, store, cx)?
    } else if let Some(node) = nid.as_ifloat_kind(store) {
        node.is_readable(device, store, cx)?
    } else if let Some(node) = nid.as_iboolean_kind(store) {
        node.is_readable(device, store, cx)?
    } else if let Some(node) = nid.as_ienumeration_kind(store) {
        node.is_readable(device, store, cx)?
    } else {
        return Err(GenApiError::invalid_node(
            format!("{}`", nid.name(store)).into(),
        ));
    })
}

pub(super) fn is_nid_writable<T: ValueStore, U: CacheStore>(
    nid: NodeId,
    device: &mut impl Device,
    store: &impl NodeStore,
    cx: &mut ValueCtxt<T, U>,
) -> GenApiResult<bool> {
    Ok(if let Some(node) = nid.as_iinteger_kind(store) {
        node.is_writable(device, store, cx)?
    } else if let Some(node) = nid.as_ifloat_kind(store) {
        node.is_writable(device, store, cx)?
    } else if let Some(node) = nid.as_iboolean_kind(store) {
        node.is_writable(device, store, cx)?
    } else if let Some(node) = nid.as_ienumeration_kind(store) {
        node.is_writable(device, store, cx)?
    } else {
        return Err(GenApiError::invalid_node(
            format!("{}`", nid.name(store)).into(),
        ));
    })
}

pub(super) fn set_eval_result<T: ValueStore, U: CacheStore>(
    nid: NodeId,
    result: EvaluationResult,
    device: &mut impl Device,
    store: &impl NodeStore,
    cx: &mut ValueCtxt<T, U>,
) -> GenApiResult<()> {
    if let Some(node) = nid.as_iinteger_kind(store) {
        node.set_value(result.as_integer(), device, store, cx)?
    } else if let Some(node) = nid.as_ifloat_kind(store) {
        node.set_value(result.as_float(), device, store, cx)?
    } else if let Some(node) = nid.as_iboolean_kind(store) {
        node.set_value(result.as_bool(), device, store, cx)?
    } else if let Some(node) = nid.as_ienumeration_kind(store) {
        node.set_entry_by_value(result.as_integer(), device, store, cx)?
    } else {
        return Err(GenApiError::invalid_node(
            format!("{}`", nid.name(store)).into(),
        ));
    }
    Ok(())
}

fn expr_from_nid<T: ValueStore, U: CacheStore>(
    nid: NodeId,
    device: &mut impl Device,
    store: &impl NodeStore,
    cx: &mut ValueCtxt<T, U>,
) -> GenApiResult<Expr> {
    Ok(if let Some(node) = nid.as_iinteger_kind(store) {
        node.value(device, store, cx)?.into()
    } else if let Some(node) = nid.as_ifloat_kind(store) {
        node.value(device, store, cx)?.into()
    } else if let Some(node) = nid.as_iboolean_kind(store) {
        node.value(device, store, cx)?.into()
    } else if let Some(node) = nid.as_ienumeration_kind(store) {
        node.current_entry(device, store, cx)
            .map(|nid| nid.expect_enum_entry(store).unwrap())?
            .numeric_value()
            .into()
    } else {
        return Err(GenApiError::invalid_node(
            format!("{}`", nid.name(store)).into(),
        ));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_from_float() {
        let mut f32_output = vec![0; 4];
        let mut f64_output = vec![0; 8];

        let value = 1024.0f64;

        bytes_from_float(value, &mut f32_output, Endianness::BE).unwrap();
        bytes_from_float(value, &mut f64_output, Endianness::BE).unwrap();

        bytes_from_float(value, &mut f32_output, Endianness::LE).unwrap();
        bytes_from_float(value, &mut f64_output, Endianness::LE).unwrap();

        assert!(bytes_from_float(value, &mut [], Endianness::LE).is_err());
    }
}
