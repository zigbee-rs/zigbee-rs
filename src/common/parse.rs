use core::iter::FromIterator;

use heapless::Vec;

pub(crate) trait PackBytes
where
    Self: Sized,
{
    fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self>;
    fn unpack_from_slice(src: &[u8]) -> Option<Self> {
        Self::unpack_from_iter(src.iter().copied())
    }
}

impl PackBytes for u8 {
    fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
        src.into_iter().next()
    }
}

impl PackBytes for i8 {
    fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
        src.into_iter().next().map(|b| b as Self)
    }
}

macro_rules! impl_primitive {
    ($ty:ty, $sz:literal) => {
        impl PackBytes for $ty {
            fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
                let buf: Vec<u8, $sz> = src.into_iter().take($sz).collect();
                Some(<$ty>::from_le_bytes(buf.into_array().unwrap()))
            }
        }
    };
}

impl_primitive!(u16, 2);
impl_primitive!(u32, 4);
impl_primitive!(u64, 8);
impl_primitive!(i16, 2);
impl_primitive!(i32, 4);
impl_primitive!(i64, 8);

impl<const N: usize> PackBytes for Vec<u8, N> {
    fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
        Some(FromIterator::from_iter(src))
    }
}

/// Implement `PackBytes` for a struct.
#[macro_export]
macro_rules! impl_pack_bytes {
    (
        $(#[$m:meta])*
        $v:vis struct $name:ident($vt:vis $ty:ty);
    ) => {
        $(#[$m])*
        $v struct $name($vt $ty);

        impl $crate::common::parse::PackBytes for $name {
            fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
                Some(Self($crate::common::parse::PackBytes::unpack_from_iter(src)?))
            }
        }
    };
    (
        $(#[$m:meta])*
        $v:vis struct $name:ident {
            $(
                $(#[doc = $doc:literal])*
                $(#[pack = $pack:literal])?
                $(#[pack_if = $pack_if:expr])?
                $(#[control_header = $ctl_hdr:ty])?
                $vf:vis $field_name:ident: $field_ty:ty
            ),+
            $(,)?
        }
    ) => {
        $(#[$m])*
        $v struct $name{
            $(
                $(#[doc = $doc])*
                $vf $field_name: $field_ty
            ),+
        }

        #[allow(unused_doc_comments)]
        impl $crate::common::parse::PackBytes for $name {
            fn unpack_from_iter(src: impl IntoIterator<Item = u8>) -> Option<Self> {
                use $crate::common::parse::PackBytes;
                let mut src = src.into_iter();
                $(
                    $(
                        let _ctl_hdr = <$ctl_hdr>::unpack_from_iter(&mut src)?;
                    )?
                )+
                Some(Self {
                    $(
                        $(
                            $field_name: {
                                let _ = $pack;
                                PackBytes::unpack_from_iter(&mut src)?
                            },
                        )?
                        $(
                            $field_name: $pack_if(&_ctl_hdr)
                                .then(|| PackBytes::unpack_from_iter(&mut src))
                                .flatten(),
                        )?
                        $(
                            $field_name: {
                                let _ = ::core::marker::PhantomData::<$ctl_hdr>{};
                                _ctl_hdr
                            },
                        )?
                    )+
                })
            }
        }
    }
}

#[macro_export]
macro_rules! impl_byte {
    (
        $(#[$m:meta])*
        $v:vis struct $name:ident($vt:vis $ty:ty);
    ) => {
        $(#[$m])*
        $v struct $name($vt $ty);

        impl<C: ::core::default::Default> ::byte::TryRead<'_, C> for $name {
            fn try_read(bytes: &'_ [u8], _: C) -> ::byte::Result<(Self, usize)> {
                let (v, sz) = <$ty>::try_read(bytes, ::byte::BE)?;
                Ok((Self(v), sz))
            }
        }

        impl<C: ::core::default::Default> ::byte::TryWrite<C> for $name {
            fn try_write(self, bytes: &mut [u8], _: C) -> ::byte::Result<usize> {
                self.0.try_write(bytes, ::byte::BE)
            }
        }
    };
    (
        $(#[$m:meta])*
        $v:vis struct $name:ident $(<$lifetime:lifetime>)? {
            $(
                $(#[doc = $doc:literal])*
                $(#[ctx = $ctx_hdr:expr])?
                $(#[parse_if = $parse_if_hdr:expr])?
                $vf:vis $field_name:ident: $field_ty:ty
            ),+
            $(,)?
        }
    ) => {
        $(#[$m])*
        $v struct $name $(<$lifetime>)? {
            $(
                $(#[doc = $doc])*
                $vf $field_name: $field_ty
            ),+
        }

        #[allow(single_use_lifetimes, unreachable_code, unused_variables)]
        impl<'a, C: ::core::default::Default> ::byte::TryRead<'a, C> for $name $(<$lifetime>)? {
            fn try_read(bytes: &'a [u8], _: C) -> ::byte::Result<(Self, usize)> {
                use ::byte::BytesExt;
                let offset = &mut 0;
                $(
                    let ctx = ::byte::BE;
                    $(
                        let ctx = $ctx_hdr;
                    )?

                    let should_read = true;
                    $(let should_read = $parse_if_hdr;)?

                    let $field_name: $field_ty = if should_read {
                        let v = bytes.read_with(offset, ctx)?;
                        $(
                            let _ = $parse_if_hdr;
                            let v = Some(v);
                        )?
                        v
                    } else {
                        (|| {
                            $(
                                let _ = $parse_if_hdr;
                                return None;
                            )?
                            unreachable!()
                        })()
                    };
                )+
                let s = Self {
                    $($field_name,)+
                };
                Ok((s, *offset))
            }
        }

        #[allow(single_use_lifetimes, unused_variables)]
        impl<'a, C: ::core::default::Default> ::byte::TryWrite<C> for $name $(<$lifetime>)? {
            fn try_write(self, bytes: &mut [u8], _: C) -> ::byte::Result<usize> {
                use ::byte::BytesExt;
                let offset = &mut 0;

                let Self {
                    $($field_name,)+
                } = self;

                $(
                    let ctx = ::byte::BE;
                    $(
                        let _ = $ctx_hdr;
                        let ctx = ();
                    )?

                    let should_write = true;
                    $(let should_write = $parse_if_hdr;)?
                    if should_write {
                        let v = $field_name;
                        $(
                            let _ = $parse_if_hdr;
                            let v = v.unwrap();
                        )?
                        bytes.write_with(offset, v, ctx)?;
                    }
                )+
                Ok(*offset)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    impl_byte! {
        pub struct DataFrame<'a> {
            #[ctx = ()]
            pub flag: bool,
            pub address: crate::common::types::ShortAddress,
            #[parse_if = flag]
            pub conditional: Option<u8>,
            #[ctx = byte::ctx::Bytes::Len(flag as usize)]
            pub data: &'a [u8],
        }
    }
}
