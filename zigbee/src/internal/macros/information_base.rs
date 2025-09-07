#[doc(hidden)]
#[macro_export]
macro_rules! construct_ib {
    (
        $(#[doc = $ib_doc:literal])*
        $ib_vis:vis struct $ib_name:ident {
            $(
                $(#[doc = $doc:literal])*
                $(#[ctx = $ctx_hdr:expr])?
                $(#[ctx_write = $ctx_write:expr])?
                $field:ident: $field_ty:path $(= $default:expr)?,
            )+
        }
    ) => {
        pub type ${ concat($ib_name, Storage) } = $crate::internal::storage::InMemoryStorage<{ ${ concat($ib_name, Id) }::BUFFER_SIZE }>;

        static mut IB: Option<$ib_name<${ concat($ib_name, Storage) }>> = None;

        /// Initializes the IB.
        pub fn init(storage: ${ concat($ib_name, Storage) }) {
            // SAFETY: NIB can only be initialized once
            unsafe {
                if IB.is_some() {
                    panic!(concat!(stringify!($ib_name), " already initialized"));
                }
                let ib = $ib_name::new(storage);
                ib.init();
                IB = Some(ib);
            }
        }

        /// Returns a reference to the NIB.
        pub fn get_ref() -> &'static $ib_name<${ concat($ib_name, Storage) }> {
            // SAFETY: NIB is mutated only in init once
            unsafe { IB.as_ref().expect(concat!(stringify!($ib_name), " not initialized")) }
        }

        #[repr(usize)]
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, Eq, PartialEq)]
        $ib_vis enum ${ concat($ib_name, Id) } {
            $($field),+
        }

        impl ${ concat($ib_name, Id) } {
            // might not be the exact size of the field
            // because encoding (produced by byte::TryWrite)
            // might be different than struct alignment
            // but `size_of` gives us an upper bound
            const IB_ID_SIZE_LUT: &[usize] = &[
                $(
                    size_of::<$field_ty>()
                ),+
            ];

            pub const BUFFER_SIZE: usize = ${ concat($ib_name, Id) }::ib_buffer_size();

            const fn ib_buffer_size() -> usize {
                let mut size = 0usize;
                let mut i = 0;
                while i < Self::IB_ID_SIZE_LUT.len() {
                    size += Self::IB_ID_SIZE_LUT[i];
                    i += 1;
                }
                size
            }

            const fn size(&self) -> usize {
                Self::IB_ID_SIZE_LUT[*self as usize]
            }

            const fn offset(&self) -> usize {
                let mut i = 0usize;
                let mut offset = 0usize;
                while i != *self as usize {
                    offset += Self::IB_ID_SIZE_LUT[i];
                    i += 1;
                }
                offset
            }
        }

        $(#[doc = $ib_doc])*
        $ib_vis struct $ib_name<C> {
            storage: ::spin::Mutex<C>,
        }

        #[allow(clippy::cast_possible_truncation)]
        impl<C: ::embedded_storage::Storage> $ib_name<C> {
            pub fn new(storage: C) -> Self {
                Self { storage: ::spin::Mutex::new(storage) }
            }

            pub fn init(&self) {
                use byte::BytesExt;
                use byte::TryRead;
                use byte::TryWrite;
                $(
                    let cx = ::byte::LE;
                    $(
                        let cx = $ctx_write;
                    )?
                    $(
                        let mut buf = [0u8; ${ concat($ib_name, Id) }::$field.size()];
                        let value: $field_ty = $default;
                        buf.write_with(&mut 0, value, cx).unwrap();
                        let _ = self.storage.lock().write(${ concat($ib_name, Id) }::$field.offset() as u32, &buf);
                    )?
                )+
            }

            $(
                $(#[doc = $doc])*
                pub fn $field(&self) -> $field_ty {
                    use byte::BytesExt;
                    use byte::TryRead;
                    use byte::TryWrite;
                    const SIZE: usize = ${ concat($ib_name, Id) }::$field.size();
                    let mut buf = [0u8; SIZE];
                    let cx = ::byte::LE;
                    $(
                        let cx = $ctx_hdr;
                    )?

                    let _ = self.storage.lock().read(${ concat($ib_name, Id) }::$field.offset() as u32, &mut buf);
                    buf.read_with(&mut 0, cx).unwrap()
                }

                pub fn ${ concat(set_, $field) }(&self, value: $field_ty) {
                    use byte::BytesExt;
                    use byte::TryRead;
                    use byte::TryWrite;
                    const SIZE: usize = ${ concat($ib_name, Id) }::$field.size();
                    let mut buf = [0u8; SIZE];

                    let cx = ::byte::LE;
                    $(
                        let cx = $ctx_write;
                    )?
                    buf.write_with(&mut 0, value, cx).unwrap();

                    let _ = self.storage.lock().write(${ concat($ib_name, Id) }::$field.offset() as u32, &buf);
                }
            )+
        }
    };
}
