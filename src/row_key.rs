//! Shared row field names. Cloning names never allocates.
use std::{borrow::Borrow, cmp::Ordering, fmt, hash::{Hash, Hasher}, sync::Arc};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Visitor};

#[derive(Clone)]
pub struct RowKey(Arc<str>);
impl RowKey {
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn into_string(self) -> String { self.as_str().to_owned() }
}
impl From<&str> for RowKey {
    #[inline]
    fn from(s: &str) -> Self {
        Self(Arc::from(s))
    }
}
impl From<String> for RowKey { fn from(s: String) -> Self { Self::from(s.as_str()) } }
impl From<Arc<str>> for RowKey {
    fn from(s: Arc<str>) -> Self { Self(s) }
}
impl From<RowKey> for String { fn from(s: RowKey) -> Self { s.into_string() } }
impl AsRef<str> for RowKey { #[inline] fn as_ref(&self) -> &str { self.as_str() } }
impl Borrow<str> for RowKey { #[inline] fn borrow(&self) -> &str { self.as_str() } }
impl std::ops::Deref for RowKey { type Target=str; fn deref(&self) -> &str { self.as_str() } }
impl PartialEq for RowKey { fn eq(&self, other: &Self) -> bool { self.as_str()==other.as_str() } }
impl Eq for RowKey {}
impl PartialOrd for RowKey { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
impl Ord for RowKey { fn cmp(&self, other: &Self) -> Ordering { self.as_str().cmp(other.as_str()) } }
impl Hash for RowKey { fn hash<H:Hasher>(&self, state: &mut H) { self.as_str().hash(state) } }
impl PartialEq<str> for RowKey { fn eq(&self, other: &str) -> bool { self.as_str()==other } }
impl PartialEq<&str> for RowKey { fn eq(&self, other: &&str) -> bool { self.as_str()==*other } }
impl PartialEq<String> for RowKey { fn eq(&self, other: &String) -> bool { self.as_str()==other } }
impl PartialEq<RowKey> for String { fn eq(&self, other: &RowKey) -> bool { self.as_str()==other.as_str() } }
impl fmt::Debug for RowKey { fn fmt(&self, f:&mut fmt::Formatter<'_>) -> fmt::Result { fmt::Debug::fmt(self.as_str(), f) } }
impl fmt::Display for RowKey { fn fmt(&self, f:&mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) } }
impl Serialize for RowKey { fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{s.serialize_str(self.as_str())} }
impl<'de> Deserialize<'de> for RowKey {
    fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{
        struct KeyVisitor;
        impl<'de> Visitor<'de> for KeyVisitor {
            type Value=RowKey;
            fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str("a UTF-8 row field name")}
            fn visit_str<E:serde::de::Error>(self,s:&str)->Result<RowKey,E>{Ok(RowKey::from(s))}
            fn visit_string<E:serde::de::Error>(self,s:String)->Result<RowKey,E>{Ok(RowKey::from(s))}
        }
        d.deserialize_str(KeyVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Row,Cell};
    use std::collections::{BTreeMap,hash_map::DefaultHasher};
    fn hash<T:Hash>(x:&T)->u64{let mut h=DefaultHasher::new();x.hash(&mut h);h.finish()}
    #[test]
    fn key_boundaries_clone_without_truncating_utf8() {
        for text in [String::new(),"a".repeat(22),"a".repeat(23),"ёж".repeat(5),"é".repeat(11),"é".repeat(12),format!("{}é","x".repeat(21))] {
            let key=RowKey::from(text.as_str());let cloned=key.clone();
            assert_eq!(key.as_str(),text);assert_eq!(cloned,key);
            assert!(Arc::ptr_eq(&key.0,&cloned.0));
        }
        #[cfg(target_pointer_width="64")]
        assert_eq!(std::mem::size_of::<RowKey>(),16);
        assert!(std::mem::size_of::<RowKey>() < std::mem::size_of::<String>());
    }
    #[test]
    fn mixed_key_storage_preserves_legacy_order_hash_and_wire_bytes() {
        let mut legacy=BTreeMap::new();
        for (i,key) in ["z", "", "α", "a very long custom metadata field name", "aa", "é", "a", "title"].into_iter().enumerate(){legacy.insert(key.to_owned(),Cell::Int(i as i64));}
        let row=Row::from(legacy.clone());
        assert_eq!(row.keys().map(|k|k.as_str()).collect::<Vec<_>>(),legacy.keys().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(hash(&row),hash(&legacy));
        assert_eq!(serde_json::to_vec(&row).unwrap(),serde_json::to_vec(&legacy).unwrap());
        let bytes=rmp_serde::to_vec_named(&row).unwrap();assert_eq!(bytes,rmp_serde::to_vec_named(&legacy).unwrap());
        assert_eq!(rmp_serde::from_slice::<Row>(&bytes).unwrap(),row);
        assert_eq!(serde_json::from_slice::<Row>(&serde_json::to_vec(&legacy).unwrap()).unwrap(),row);
        for key in legacy.keys(){assert_eq!(row.get(key),legacy.get(key));assert_eq!(row.get(key.as_str()),legacy.get(key));}
        let back:BTreeMap<String,Cell>=row.into();assert_eq!(back,legacy);
    }
    #[test]
    fn shared_row_patch_preserves_held_values_and_logical_quota_charge() {
        let mut row=Row::from([("id".into(),Cell::Int(7)),("a very long custom field name".into(),Cell::Float(-0.0)),("title".into(),Cell::from("original"))]);
        row.ensure_shared();let held=row.clone();let held_bytes=rmp_serde::to_vec_named(&held).unwrap();
        let before=crate::store::row_byte_size(&row);
        row.insert_str("title",Cell::from("modified"));
        assert_eq!(row.get("title").and_then(Cell::text),Some("modified"));
        assert_eq!(held.get("title").and_then(Cell::text),Some("original"));
        assert_eq!(rmp_serde::to_vec_named(&held).unwrap(),held_bytes);
        assert_eq!(crate::store::row_byte_size(&held),before);
        assert_eq!(held.get("a very long custom field name").and_then(|v|if let Cell::Float(f)=v{Some(f.to_bits())}else{None}),Some((-0.0f64).to_bits()));
        let charge=crate::store::row_byte_size(&row);row.ensure_shared();assert_eq!(crate::store::row_byte_size(&row),charge);
    }
}
