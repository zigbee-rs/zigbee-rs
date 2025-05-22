/// Implements `byte` for a struct.
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
                let (v, sz) = <$ty>::try_read(bytes, ::byte::LE)?;
                Ok((Self(v), sz))
            }
        }

        impl<C: ::core::default::Default> ::byte::TryWrite<C> for $name {
            fn try_write(self, bytes: &mut [u8], _: C) -> ::byte::Result<usize> {
                self.0.try_write(bytes, ::byte::LE)
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

        #[allow(single_use_lifetimes, clippy::redundant_closure_call, unreachable_code, unused_variables)]
        impl<'a, C: ::core::default::Default> ::byte::TryRead<'a, C> for $name $(<$lifetime>)? {
            fn try_read(bytes: &'a [u8], _: C) -> ::byte::Result<(Self, usize)> {
                use ::byte::BytesExt;
                let offset = &mut 0;
                $(
                    let ctx = ::byte::LE;
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
                    let ctx = ::byte::LE;
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
    use byte::TryRead;
    use byte::TryWrite;

    use crate::common::types::ShortAddress;

    impl_byte! {
        struct DataFrame<'a> {
            flag: u8,
            address: ShortAddress,
            #[parse_if = flag > 0]
            opt: Option<u16>,
            length: u8,
            #[ctx = byte::ctx::Bytes::Len(usize::from(length))]
            data: &'a [u8],
        }
    }

    #[test]
    fn parse() {
        let bytes = &[0x01, 0x12, 0xff, 0x11, 0x22, 0x4, 0xaa, 0xaa, 0xaa, 0xaa];

        let (frame, len) = DataFrame::try_read(bytes, ()).unwrap();

        assert_eq!(len, 10);
        assert_eq!(frame.flag, 0x01);
        assert_eq!(frame.address, ShortAddress(0xff12));
        assert_eq!(frame.opt, Some(0x2211));
        assert_eq!(frame.length, 0x04);
        assert_eq!(frame.data, &[0xaa, 0xaa, 0xaa, 0xaa]);

        let mut buf = [0u8; 10];
        frame.try_write(&mut buf, ()).unwrap();
        assert_eq!(&buf, bytes);
    }
}
