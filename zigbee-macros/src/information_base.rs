#[doc(hidden)]
#[macro_export]
macro_rules! construct_ib {
    (@default $default:expr) => { $default };
    (@default) => { ::core::default::Default::default() };
    (
        $(#[doc = $ib_doc:literal])*
        $ib_vis:vis struct $ib_name:ident {
            $(
                $(#[doc = $doc:literal])*
                $(#[ctx = $ctx_hdr:expr])?
                $(#[ctx_write = $ctx_write:expr])?
                $(#[storage_key = $skey:literal])?
                $field:ident: $field_ty:path $(= $default:expr)?,
            )+
        }
    ) => {
        static mut IB: Option<$ib_name> = None;

        /// Initializes the IB with default values.
        pub fn init() {
            // SAFETY: the IB can only be initialized once
            unsafe {
                if IB.is_some() {
                    panic!(concat!(stringify!($ib_name), " already initialized"));
                }
                IB = Some($ib_name::new());
            }
        }

        /// Returns a reference to the IB.
        pub fn get_ref() -> &'static $ib_name {
            // SAFETY: IB is mutated only in init once
            unsafe { IB.as_ref().expect(concat!(stringify!($ib_name), " not initialized")) }
        }

        static DIRTY_SIGNAL: ::zigbee_types::sync::Event =
            ::zigbee_types::sync::Event::new();

        /// Waits until a persisted attribute changes (single waiter).
        ///
        /// Wakes once per batch of changes; pair with `take_dirty` to see
        /// which fields changed.
        pub async fn changed() {
            DIRTY_SIGNAL.wait().await;
        }

        /// Initialize the IB if not already initialized, otherwise do nothing.
        #[cfg(test)]
        pub fn try_init() {
            // SAFETY: only called at test setup before concurrent access
            unsafe {
                if IB.is_none() {
                    IB = Some($ib_name::new());
                }
            }
        }

        /// Re-write default values to the existing IB.
        #[cfg(test)]
        pub fn reset() {
            // SAFETY: only called at test setup; the singleton reference
            // is not reallocated, just its contents are overwritten
            unsafe {
                if let Some(ref ib) = IB {
                    $(
                        *ib.fields.$field.write() =
                            $crate::construct_ib!(@default $($default)?);
                    )+
                    *ib.dirty.lock() = 0;
                }
            }
        }

        #[repr(usize)]
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, Debug, Eq, PartialEq)]
        $ib_vis enum ${ concat($ib_name, Id) } {
            $($field),+
        }

        impl ${ concat($ib_name, Id) } {
            // upper bound of the `byte`-encoded size per field: encodings are
            // packed and never larger than the in-memory representation
            const ENCODED_SIZE_LUT: &[usize] = &[
                $(
                    size_of::<$field_ty>()
                ),+
            ];

            // stable persistence key per field; None = RAM-only field
            const STORAGE_KEY_LUT: &[Option<u8>] = &[
                $(
                    {
                        let key: Option<u8> = None;
                        $(let key = Some($skey);)?
                        key
                    }
                ),+
            ];

            /// Upper bound of the encoded size over all persisted fields.
            pub const MAX_FIELD_SIZE: usize = ${ concat($ib_name, Id) }::max_field_size();

            /// All field ids in declaration order.
            pub const VARIANTS: &[Self] = &[$(Self::$field),+];

            /// Returns the stable persistence key, or `None` for RAM-only fields.
            pub const fn storage_key(&self) -> Option<u8> {
                Self::STORAGE_KEY_LUT[*self as usize]
            }

            /// Resolves a stable persistence key back to its field id.
            pub const fn from_storage_key(key: u8) -> Option<Self> {
                $(
                    if let Some(k) = Self::STORAGE_KEY_LUT[Self::$field as usize] {
                        if k == key {
                            return Some(Self::$field);
                        }
                    }
                )+
                None
            }

            const fn max_field_size() -> usize {
                let mut max = 0usize;
                let mut i = 0;
                while i < Self::ENCODED_SIZE_LUT.len() {
                    if Self::STORAGE_KEY_LUT[i].is_some() && Self::ENCODED_SIZE_LUT[i] > max {
                        max = Self::ENCODED_SIZE_LUT[i];
                    }
                    i += 1;
                }
                max
            }

            const fn storage_keys_unique() -> bool {
                let lut = Self::STORAGE_KEY_LUT;
                let mut i = 0;
                while i < lut.len() {
                    let mut j = i + 1;
                    while j < lut.len() {
                        if let (Some(a), Some(b)) = (lut[i], lut[j]) {
                            if a == b {
                                return false;
                            }
                        }
                        j += 1;
                    }
                    i += 1;
                }
                true
            }
        }

        const _: () = {
            // dirty mask is a u64 bitmask indexed by field position
            assert!(${ concat($ib_name, Id) }::STORAGE_KEY_LUT.len() <= 64);
            assert!(${ concat($ib_name, Id) }::storage_keys_unique());
        };

        // plain in-memory representation with one lock per field so readers
        // never contend with each other; serialization only happens when a
        // field is exported to / imported from persistent storage
        #[allow(non_camel_case_types)]
        struct ${ concat($ib_name, Fields) } {
            $($field: ::spin::RwLock<$field_ty>,)+
        }

        impl ${ concat($ib_name, Fields) } {
            fn defaults() -> Self {
                Self {
                    $($field: ::spin::RwLock::new($crate::construct_ib!(@default $($default)?)),)+
                }
            }
        }

        $(#[doc = $ib_doc])*
        $ib_vis struct $ib_name {
            fields: ${ concat($ib_name, Fields) },
            // bitmask of persisted fields modified since the last take_dirty
            dirty: ::spin::Mutex<u64>,
        }

        impl $ib_name {
            pub fn new() -> Self {
                Self {
                    fields: ${ concat($ib_name, Fields) }::defaults(),
                    dirty: ::spin::Mutex::new(0),
                }
            }

            /// Returns and clears the bitmask of fields modified since the
            /// last call.
            pub fn take_dirty(&self) -> u64 {
                let mut dirty = self.dirty.lock();
                core::mem::take(&mut *dirty)
            }

            /// Re-arms the dirty bit of a field, e.g. after a failed store.
            pub fn mark_dirty(&self, id: ${ concat($ib_name, Id) }) {
                *self.dirty.lock() |= 1 << (id as u64);
                DIRTY_SIGNAL.signal();
            }

            /// Encodes a single field into `buf`, returning the encoded length.
            ///
            /// Returns `None` for RAM-only fields (no storage key) or if `buf`
            /// is too small.
            pub fn export_field(&self, id: ${ concat($ib_name, Id) }, buf: &mut [u8]) -> Option<usize> {
                use byte::BytesExt;
                use byte::TryWrite;
                id.storage_key()?;
                match id {
                    $(
                        ${ concat($ib_name, Id) }::$field => {
                            let value: $field_ty =
                                ::core::clone::Clone::clone(&*self.fields.$field.read());
                            let _cx = ::byte::LE;
                            $(
                                let _cx = $ctx_write;
                            )?
                            let mut offset = 0;
                            buf.write_with(&mut offset, value, _cx).ok()?;
                            Some(offset)
                        }
                    )+
                }
            }

            /// Decodes `data` into a single field without marking it dirty.
            ///
            /// Returns `false` if the data does not parse as the field type,
            /// leaving the current value untouched.
            pub fn import_field(&self, id: ${ concat($ib_name, Id) }, data: &[u8]) -> bool {
                use byte::BytesExt;
                use byte::TryRead;
                match id {
                    $(
                        ${ concat($ib_name, Id) }::$field => {
                            let _cx = ::byte::LE;
                            $(
                                let _cx = $ctx_hdr;
                            )?
                            let Ok(value) = data.read_with::<$field_ty>(&mut 0, _cx) else {
                                return false;
                            };
                            *self.fields.$field.write() = value;
                            true
                        }
                    )+
                }
            }

            $(
                $(#[doc = $doc])*
                ///
                /// Returns a read guard; do not hold it across an
                /// `update_*` of the same field.
                pub fn $field(&self) -> ::spin::RwLockReadGuard<'_, $field_ty> {
                    self.fields.$field.read()
                }

                /// Updates the field in place under its write lock.
                pub fn ${ concat(update_, $field) }(&self, f: impl FnOnce(&mut $field_ty)) {
                    f(&mut *self.fields.$field.write());

                    if ${ concat($ib_name, Id) }::$field.storage_key().is_some() {
                        *self.dirty.lock() |= 1 << (${ concat($ib_name, Id) }::$field as u64);
                        DIRTY_SIGNAL.signal();
                    }
                }
            )+
        }
    };
}
