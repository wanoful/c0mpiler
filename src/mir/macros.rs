macro_rules! rewrite_field {
    (Reg, rd, $val:ident, $use_rewrites:expr, $def_rewrites:expr) => {
        match $val {
            Register::Virtual(vreg) => {
                if let Some(reg) = $def_rewrites.get(&vreg) {
                    *reg
                } else {
                    *$val
                }
            }
            Register::Physical(reg) => Register::Physical(*reg),
        }
    };
    (Reg, $field:ident, $val:ident, $use_rewrites:expr, $def_rewrites:expr) => {
        match $val {
            Register::Virtual(vreg) => {
                if let Some(reg) = $use_rewrites.get(&vreg) {
                    *reg
                } else {
                    *$val
                }
            }
            Register::Physical(reg) => Register::Physical(*reg),
        }
    };
    ($ty:ident, $field:ident, $val:ident, $use_rewrites:expr, $def_rewrites:expr) => {
        *$val
    };
}

macro_rules! generate_reg_rewrite {
    (
        $(#[$meta:meta])*
        $vis:vis enum $enum_name:ident {
            $(
                $variant:ident $({ $( $field:ident : $field_ty:ident ),* $(,)? })?
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $enum_name {
            $(
                $variant $({ $( $field : $field_ty ),* })?
            ),*
        }

        impl $enum_name {
            $vis fn rewrite_vreg(
                &self,
                use_rewrites: &std::collections::HashMap<crate::mir::VRegId, Reg>,
                def_rewrites: &std::collections::HashMap<crate::mir::VRegId, Reg>,
            ) -> Self
            where
                Self: Sized,
            {
                match self {
                    $(
                        Self::$variant $({ $( $field ),* })? => {
                            Self::$variant $({
                                $( $field: $crate::mir::macros::rewrite_field!($field_ty, $field, $field, use_rewrites, def_rewrites) ),*
                            })?
                        }
                    ),*
                }
            }
        }
    }
}

pub(crate) use generate_reg_rewrite;
pub(crate) use rewrite_field;