use crate::{
    impossible,
    ir::ir_type::{StructTypePtr, TypePtr},
    irgen::IRGenerator,
    semantics::resolved_ty::{AnyTyKind, BuiltInTyKind, ResolvedTy, ResolvedTyKind, TypeIntern},
};

/*
    转换方式：
    1. Faithful：除了 never type 转换为 void type 外，其他原样转换
        复合类型（tup, array）内部始终使用 Faithful 转换
    2. FirstClass: never type -> void type, 简单类型原样转换，复合类型 -> ptr type
        函数参数使用 FirstClass 转换
    2. FirstClassNoUnit：never type, unit type -> void type，简单类型原样转换，复合类型 -> ptr type
        函数返回值使用 FirstClassNoUnit 转换
*/
pub(crate) enum TransformTypeConfig {
    Faithful,
    FirstClass,
    FirstClassNoUnit,
}

impl TransformTypeConfig {
    fn no_aggregate_type(&self) -> bool {
        matches!(self, Self::FirstClass | Self::FirstClassNoUnit)
    }

    fn no_unit(&self) -> bool {
        matches!(self, Self::FirstClassNoUnit)
    }
}

impl<'ast, 'analyzer> IRGenerator<'ast, 'analyzer> {
    pub(crate) fn is_aggregate_type(&self, x: &ResolvedTy) -> bool {
        matches!(
            x.kind,
            ResolvedTyKind::Array(..) | ResolvedTyKind::Fn(..) | ResolvedTyKind::Tup(..)
        )
    }

    pub(crate) fn is_zero_length_type(&self, x: &ResolvedTy) -> bool {
        match &x.kind {
            ResolvedTyKind::Tup(items) => items.is_empty(),
            _ => false,
        }
    }

    pub(crate) fn fat_ptr_type(&self) -> StructTypePtr {
        self.context.get_named_struct_type("fat_ptr").unwrap()
    }

    pub(crate) fn transform_interned_ty_faithfully(&self, intern: TypeIntern) -> TypePtr {
        self.transform_interned_ty_impl(intern, TransformTypeConfig::Faithful)
    }

    pub(crate) fn transform_ty_faithfully(&self, ty: &ResolvedTy) -> TypePtr {
        self.transform_ty_impl(ty, TransformTypeConfig::Faithful)
    }

    pub(crate) fn transform_interned_ty_impl(
        &self,
        intern: TypeIntern,
        cfg: TransformTypeConfig,
    ) -> TypePtr {
        let Some(ty) = self.analyzer.probe_type(intern) else {
            return self.context.void_type().into();
        };
        self.transform_ty_impl(&ty, cfg)
    }

    // 空长度类型怎么办？
    pub(crate) fn transform_ty_impl(&self, ty: &ResolvedTy, cfg: TransformTypeConfig) -> TypePtr {
        if cfg.no_unit() && self.is_zero_length_type(ty) {
            return self.context.void_type().into();
        }
        if cfg.no_aggregate_type() && self.is_aggregate_type(ty) {
            return self.context.ptr_type().into();
        }
        use crate::semantics::resolved_ty::ResolvedTyKind::*;
        if matches!(ty.kind, Placeholder | Tup(..))
            && let Some((name, _)) = &ty.names
        {
            return self
                .context
                .get_named_struct_type(&name.to_string())
                .unwrap()
                .into();
        }
        match &ty.kind {
            Placeholder => {
                impossible!()
            }
            BuiltIn(built_in_ty_kind) => match built_in_ty_kind {
                BuiltInTyKind::Bool => self.context.i1_type().into(),
                BuiltInTyKind::Char => self.context.i8_type().into(),
                BuiltInTyKind::I32 | BuiltInTyKind::U32 => self.context.i32_type().into(),
                BuiltInTyKind::ISize | BuiltInTyKind::USize => {
                    self.context.ptr_width_int_type().into()
                }
                BuiltInTyKind::Str => impossible!(),
            },
            Ref(inner, _) => {
                let inner_ty = self.analyzer.probe_type(*inner).unwrap();
                if inner_ty.is_unsized_type() {
                    self.fat_ptr_type().into()
                } else {
                    self.context.ptr_type().into()
                }
            }
            Tup(items) => {
                if let Some((name, _)) = &ty.names {
                    self.context
                        .get_named_struct_type(&name.to_string())
                        .unwrap()
                        .into()
                } else if items.is_empty() && cfg.no_unit() {
                    self.context.void_type().into()
                } else {
                    self.context
                        .struct_type(
                            items
                                .iter()
                                .map(|x| {
                                    self.transform_interned_ty_impl(
                                        *x,
                                        TransformTypeConfig::Faithful,
                                    )
                                })
                                .collect(),
                            false,
                        )
                        .into()
                }
            }
            Enum => self.context.i32_type().into(),
            Trait => impossible!(),
            Array(inner, len) => self
                .context
                .array_type(
                    self.transform_interned_ty_impl(*inner, TransformTypeConfig::Faithful),
                    len.unwrap(),
                )
                .into(),
            Fn(..) => self.context.ptr_type().into(), // 即使在数组或结构体中，function 也是个指针
            Any(AnyTyKind::Any) => impossible!(),
            Any(AnyTyKind::AnyInt) | Any(AnyTyKind::AnySignedInt) => self.context.i32_type().into(),
            ImplicitSelf(_) => impossible!(),
        }
    }
}
