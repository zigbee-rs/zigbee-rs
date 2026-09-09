#[doc(hidden)]
#[macro_export]
macro_rules! construct_ib {
    (@default $default:expr) => { $default };
    (@default) => { ::core::default::Default::default() };
    // how a field is stored: behind a lock by default, in a plain atomic
    // when declared `#[cell = atomic]`
    (@cell $ty:path; [] []) => { ::spin::RwLock<$ty> };
    (@cell $ty:path; [atomic] []) => { <$ty as ::zigbee_types::sync::AtomicCell>::Cell };
    (@cell $ty:path; [] [$table:ident]) => { ::zigbee_types::sync::TableCell<$ty> };
    (@ref $ty:path; [] []) => { ::spin::RwLockReadGuard<'_, $ty> };
    (@ref $ty:path; [atomic] []) => { $ty };
    (@ref $ty:path; [] [$table:ident]) => { ::spin::RwLockReadGuard<'_, $ty> };
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
    // entry-level access for `#[table = ...]` fields; a field must have both a
    // storage key and the table marker to take part
    (@entry_export $s:ident, $id:ident, $i:ident, $buf:ident, $field:ident, $ty:path; [] [$($t:ident)?] [$($cx:expr)?]) => {};
    (@entry_export $s:ident, $id:ident, $i:ident, $buf:ident, $field:ident, $ty:path; [$skey:literal] [] [$($cx:expr)?]) => {};
    (@entry_export $s:ident, $id:ident, $i:ident, $buf:ident, $field:ident, $ty:path; [$skey:literal] [$t:ident] [$($cx:expr)?]) => {
        if $id as u8 == $skey {
            let table = ::zigbee_types::sync::IbCell::get(&$s.fields.$field);
            let entry = ::core::clone::Clone::clone(
                ::zigbee_types::sync::Table::get(&*table, $i)?
            );
            let _cx = ::byte::LE;
            $(let _cx = $cx;)?
            let mut offset = 0;
            $buf.write_with(&mut offset, entry, _cx).ok()?;
            return Some(offset);
        }
    };
    (@entry_import $s:ident, $id:ident, $i:ident, $data:ident, $field:ident, $ty:path; [] [$($t:ident)?] [$($cx:expr)?]) => {};
    (@entry_import $s:ident, $id:ident, $i:ident, $data:ident, $field:ident, $ty:path; [$skey:literal] [] [$($cx:expr)?]) => {};
    (@entry_import $s:ident, $id:ident, $i:ident, $data:ident, $field:ident, $ty:path; [$skey:literal] [$t:ident] [$($cx:expr)?]) => {
        if $id as u8 == $skey {
            let _cx = ::byte::LE;
            $(let _cx = $cx;)?
            type Entry = <$ty as ::zigbee_types::sync::Table>::Entry;
            let Ok(entry) = $data.read_with::<Entry>(&mut 0, _cx) else {
                return false;
            };
            let mut table = $s.fields.$field.table_mut(&DIRTY_SIGNAL);
            if $i < table.len() {
                table.update($i, |slot| *slot = entry);
            } else if $i == table.len() {
                let _ = table.push(entry);
            }
            return true;
        }
    };
    (@table_len $s:ident, $id:ident, $field:ident; [] [$($t:ident)?]) => {};
    (@table_len $s:ident, $id:ident, $field:ident; [$skey:literal] []) => {};
    (@table_len $s:ident, $id:ident, $field:ident; [$skey:literal] [$t:ident]) => {
        if $id as u8 == $skey {
            let table = ::zigbee_types::sync::IbCell::get(&$s.fields.$field);
            return Some(::zigbee_types::sync::Table::len(&*table));
        }
    };
    (@table_truncate $s:ident, $id:ident, $len:ident, $field:ident; [] [$($t:ident)?]) => {};
    (@table_truncate $s:ident, $id:ident, $len:ident, $field:ident; [$skey:literal] []) => {};
    (@table_truncate $s:ident, $id:ident, $len:ident, $field:ident; [$skey:literal] [$t:ident]) => {
        if $id as u8 == $skey {
            let mut table = $s.fields.$field.table_mut(&DIRTY_SIGNAL);
            while table.len() > $len {
                table.remove(table.len() - 1);
            }
            return;
        }
    };
    (@len_dirty $s:ident, $id:ident, $field:ident; [] [$($t:ident)?]) => {};
    (@len_dirty $s:ident, $id:ident, $field:ident; [$skey:literal] []) => {};
    (@len_dirty $s:ident, $id:ident, $field:ident; [$skey:literal] [$t:ident]) => {
        if $id as u8 == $skey {
            return $s.fields.$field.take_len_dirty();
        }
    };
    (@entry_dirty $s:ident, $id:ident, $field:ident; [] [$($t:ident)?]) => {};
    (@entry_dirty $s:ident, $id:ident, $field:ident; [$skey:literal] []) => {};
    (@entry_dirty $s:ident, $id:ident, $field:ident; [$skey:literal] [$t:ident]) => {
        if $id as u8 == $skey {
            return $s.fields.$field.take_dirty_entries();
        }
    };
    // encoded-size bounds: whole-field items and single table entries are
    // sized separately because they live in different map items
    (@field_size $ty:path; [] [$($t:ident)?]) => { 0usize };
    (@field_size $ty:path; [$skey:literal] [$t:ident]) => { 0usize };
    (@field_size $ty:path; [$skey:literal] []) => { size_of::<$ty>() };
    (@entry_size $ty:path; [] [$($t:ident)?]) => { 0usize };
    (@entry_size $ty:path; [$skey:literal] []) => { 0usize };
    (@entry_size $ty:path; [$skey:literal] [$t:ident]) => {
        size_of::<<$ty as ::zigbee_types::sync::Table>::Entry>()
    };
    (
        $(#[doc = $ib_doc:literal])*
        #[ids = $ib_id:ident]
        #[fields = $ib_fields:ident]
        $ib_vis:vis struct $ib_name:ident {
            $(
                $(#[doc = $doc:literal])*
                $(#[cell = $cell:ident])?
                $(#[table = $table:ident])?
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
                $(
                    let size = $crate::construct_ib!(
                        @field_size $field_ty; [$($skey)?] [$($table)?]
                    );
                    if size > max {
                        max = size;
                    }
                )+
                max
            };

            /// Upper bound of the encoded size of a single table entry.
            pub const MAX_ENTRY_SIZE: usize = {
                let mut max = 0usize;
                $(
                    let size = $crate::construct_ib!(
                        @entry_size $field_ty; [$($skey)?] [$($table)?]
                    );
                    if size > max {
                        max = size;
                    }
                )+
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
            $($field: $crate::construct_ib!(@cell $field_ty; [$($cell)?] [$($table)?]),)+
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

            /// Clears the dirty bit of a field, for a change the stored image
            /// does not reflect.
            ///
            /// Only sound when this field has a single writer, which must
            /// re-mark it once the stored image would actually change.
            pub fn unmark_dirty(&self, id: $ib_id) {
                self.dirty.clear(id.storage_key());
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

            /// Number of entries in a table field, `None` for other fields.
            pub fn table_len(&self, id: $ib_id) -> Option<usize> {
                $(
                    $crate::construct_ib!(
                        @table_len self, id, $field; [$($skey)?] [$($table)?]
                    );
                )+
                None
            }

            /// Drops table entries beyond `len`.
            pub fn truncate_table(&self, id: $ib_id, len: usize) {
                $(
                    $crate::construct_ib!(
                        @table_truncate self, id, len, $field; [$($skey)?] [$($table)?]
                    );
                )+
            }

            /// Returns and clears the set of table entries changed since the
            /// last call.
            pub fn take_dirty_entries(&self, id: $ib_id) -> u64 {
                $(
                    $crate::construct_ib!(
                        @entry_dirty self, id, $field; [$($skey)?] [$($table)?]
                    );
                )+
                0
            }

            /// Returns and clears whether a table field gained or lost
            /// entries since the last call.
            pub fn take_len_dirty(&self, id: $ib_id) -> bool {
                $(
                    $crate::construct_ib!(
                        @len_dirty self, id, $field; [$($skey)?] [$($table)?]
                    );
                )+
                false
            }

            /// Encodes one table entry into `buf`, returning the encoded
            /// length.
            pub fn export_entry(
                &self,
                id: $ib_id,
                index: usize,
                buf: &mut [u8],
            ) -> Option<usize> {
                use byte::BytesExt;
                use byte::TryWrite;
                $(
                    $crate::construct_ib!(
                        @entry_export self, id, index, buf, $field, $field_ty;
                        [$($skey)?] [$($table)?] [$($ctx_write)?]
                    );
                )+
                None
            }

            /// Decodes `data` into the table entry at `index`, appending when
            /// it is one past the end.
            pub fn import_entry(&self, id: $ib_id, index: usize, data: &[u8]) -> bool {
                use byte::BytesExt;
                use byte::TryRead;
                $(
                    $crate::construct_ib!(
                        @entry_import self, id, index, data, $field, $field_ty;
                        [$($skey)?] [$($table)?] [$($ctx_hdr)?]
                    );
                )+
                false
            }

            $(
                $(#[doc = $doc])*
                ///
                /// Locked fields return a read guard, atomic fields a copy;
                /// never hold a guard across an `update_*` of the same field.
                pub fn $field(&self) -> $crate::construct_ib!(@ref $field_ty; [$($cell)?] [$($table)?]) {
                    ::zigbee_types::sync::IbCell::get(&self.fields.$field)
                }

                $(
                    /// Borrows the table for entry-level mutation; only the
                    /// entries actually touched are persisted.
                    pub fn $table(&self) -> ::zigbee_types::sync::TableMut<'_, $field_ty> {
                        self.fields.$field.table_mut(&DIRTY_SIGNAL)
                    }
                )?

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
