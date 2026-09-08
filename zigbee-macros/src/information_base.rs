#[doc(hidden)]
#[macro_export]
macro_rules! construct_ib {
    (@default $default:expr) => { $default };
    (@default) => { ::core::default::Default::default() };
    // how a field is stored: behind a lock by default, in a plain atomic
    // when declared `#[cell = atomic]`
    (@cell $ty:path; []) => { ::spin::RwLock<$ty> };
    (@cell $ty:path; [atomic]) => { <$ty as ::zigbee_types::sync::AtomicCell>::Cell };
    (@ref $ty:path; []) => { ::spin::RwLockReadGuard<'_, $ty> };
    (@ref $ty:path; [atomic]) => { $ty };
    // per-field encode/decode; RAM-only fields (no storage key) expand to
    // nothing. The optional key and the optional byte context cannot be
    // nested in one repetition, hence the split into these rules
    (@export $s:ident, $id:ident, $buf:ident, $field:ident, $ty:path; [] [$($cx:expr)?]) => {};
    (@export $s:ident, $id:ident, $buf:ident, $field:ident, $ty:path; [$skey:literal] [$($cx:expr)?]) => {
        if $id as u8 == $skey {
            let value: $ty = ::zigbee_types::sync::IbCell::get_owned(&$s.fields.$field);
            let _cx = ::byte::LE;
            $(let _cx = $cx;)?
            let mut offset = 0;
            $buf.write_with(&mut offset, value, _cx).ok()?;
            return Some(offset);
        }
    };
    (@import $s:ident, $id:ident, $data:ident, $field:ident, $ty:path; [] [$($cx:expr)?]) => {};
    (@import $s:ident, $id:ident, $data:ident, $field:ident, $ty:path; [$skey:literal] [$($cx:expr)?]) => {
        if $id as u8 == $skey {
            let _cx = ::byte::LE;
            $(let _cx = $cx;)?
            let Ok(value) = $data.read_with::<$ty>(&mut 0, _cx) else {
                return false;
            };
            ::zigbee_types::sync::IbCell::set(&$s.fields.$field, value);
            return true;
        }
    };
    (
        $(#[doc = $ib_doc:literal])*
        #[ids = $ib_id:ident]
        #[fields = $ib_fields:ident]
        $ib_vis:vis struct $ib_name:ident {
            $(
                $(#[doc = $doc:literal])*
                $(#[cell = $cell:ident])?
                $(#[ctx = $ctx_hdr:expr])?
                $(#[ctx_write = $ctx_write:expr])?
                $(#[storage_key = $skey:literal])?
                #[setter = $update:ident]
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
                        ::zigbee_types::sync::IbCell::set(
                            &ib.fields.$field,
                            $crate::construct_ib!(@default $($default)?),
                        );
                    )+
                    let _ = ib.dirty.take();
                }
            }
        }

        // the discriminant is the stable persistence key; RAM-only fields
        // have no id
        #[repr(u8)]
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, Debug, Eq, PartialEq)]
        $ib_vis enum $ib_id {
            $($($field = $skey,)?)+
        }

        impl $ib_id {
            /// Highest storage key in use.
            pub const MAX_KEY: u8 = {
                let mut max = 0u8;
                $($(if $skey > max { max = $skey; })?)+
                max
            };

            /// Upper bound of the encoded size over all persisted fields.
            ///
            /// `byte` encodings are packed and never larger than the
            /// in-memory representation.
            pub const MAX_FIELD_SIZE: usize = {
                let mut max = 0usize;
                $($(
                    let _ = $skey;
                    if size_of::<$field_ty>() > max {
                        max = size_of::<$field_ty>();
                    }
                )?)+
                max
            };

            /// Stable persistence key of this field.
            pub const fn storage_key(&self) -> u8 {
                *self as u8
            }

            /// Bit of this field in the dirty mask.
            pub const fn bit(&self) -> u64 {
                1 << (*self as u64)
            }

            /// Resolves a stable persistence key back to its field id.
            pub const fn from_storage_key(key: u8) -> Option<Self> {
                match key {
                    $($($skey => Some(Self::$field),)?)+
                    _ => None,
                }
            }
        }

        const _: () = {
            // dirty mask is a u64 bitmask indexed by storage key
            assert!($ib_id::MAX_KEY < 64, "information base cannot have more than 64 keys");
        };

        // plain in-memory representation with one lock per field so readers
        // never contend with each other; serialization only happens when a
        // field is exported to / imported from persistent storage
        #[allow(non_camel_case_types)]
        struct $ib_fields {
            $($field: $crate::construct_ib!(@cell $field_ty; [$($cell)?]),)+
        }

        impl $ib_fields {
            fn defaults() -> Self {
                Self {
                    $($field: ::zigbee_types::sync::IbCell::new(
                        $crate::construct_ib!(@default $($default)?)
                    ),)+
                }
            }
        }

        $(#[doc = $ib_doc])*
        $ib_vis struct $ib_name {
            fields: $ib_fields,
            // persisted fields modified since the last take_dirty, indexed
            // by storage key
            dirty: ::zigbee_types::sync::BitSet64,
        }

        impl $ib_name {
            pub fn new() -> Self {
                Self {
                    fields: $ib_fields::defaults(),
                    dirty: ::zigbee_types::sync::BitSet64::new(),
                }
            }

            /// Returns and clears the bitmask of fields modified since the
            /// last call.
            pub fn take_dirty(&self) -> u64 {
                self.dirty.take()
            }

            /// Re-arms the dirty bit of a field, e.g. after a failed store.
            pub fn mark_dirty(&self, id: $ib_id) {
                self.dirty.set(id.storage_key());
                DIRTY_SIGNAL.signal();
            }

            /// Encodes a single field into `buf`, returning the encoded length.
            ///
            /// Returns `None` if `buf` is too small.
            pub fn export_field(&self, id: $ib_id, buf: &mut [u8]) -> Option<usize> {
                use byte::BytesExt;
                use byte::TryWrite;
                $(
                    $crate::construct_ib!(
                        @export self, id, buf, $field, $field_ty;
                        [$($skey)?] [$($ctx_write)?]
                    );
                )+
                None
            }

            /// Decodes `data` into a single field without marking it dirty.
            ///
            /// Returns `false` if the data does not parse as the field type,
            /// leaving the current value untouched.
            pub fn import_field(&self, id: $ib_id, data: &[u8]) -> bool {
                use byte::BytesExt;
                use byte::TryRead;
                $(
                    $crate::construct_ib!(
                        @import self, id, data, $field, $field_ty;
                        [$($skey)?] [$($ctx_hdr)?]
                    );
                )+
                false
            }

            $(
                $(#[doc = $doc])*
                ///
                /// Locked fields return a read guard, atomic fields a copy;
                /// never hold a guard across an `update_*` of the same field.
                pub fn $field(&self) -> $crate::construct_ib!(@ref $field_ty; [$($cell)?]) {
                    ::zigbee_types::sync::IbCell::get(&self.fields.$field)
                }

                /// Updates the field in place.
                pub fn $update(&self, f: impl FnOnce(&mut $field_ty)) {
                    ::zigbee_types::sync::IbCell::update(&self.fields.$field, f);

                    $(
                        self.dirty.set($skey);
                        DIRTY_SIGNAL.signal();
                    )?
                }
            )+
        }
    };
}
