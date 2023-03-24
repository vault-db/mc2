use std::cell::RefCell;
use std::collections::BTreeMap;

type Rev = usize;

#[derive(Clone)]
pub struct Store<T> {
    data: BTreeMap<String, (Rev, Option<T>)>,
    pub seq: Rev,
}

impl<T> Store<T>
where
    T: Clone,
{
    pub fn new() -> Store<T> {
        Store {
            data: BTreeMap::new(),
            seq: 0,
        }
    }

    pub fn read(&self, key: &str) -> Option<(Rev, T)> {
        if let Some((rev, Some(value))) = self.data.get(key) {
            Some((*rev, value.clone()))
        } else {
            None
        }
    }

    pub fn write(&mut self, key: &str, rev: Option<Rev>, value: T) -> Option<Rev> {
        self.set_key(key, rev, Some(value))
    }

    fn set_key(&mut self, key: &str, rev: Option<Rev>, value: Option<T>) -> Option<Rev> {
        let client_rev = rev.unwrap_or(0);
        let entry = self.data.entry(key.into()).or_insert((0, None));

        if entry.0 != client_rev {
            return None;
        }

        *entry = (entry.0 + 1, value);
        self.seq += 1;

        Some(entry.0)
    }

    pub fn keys(&self) -> Vec<&str> {
        self.data.keys().map(|key| key.as_ref()).collect()
    }
}

pub struct Cache<'a, T> {
    store: &'a RefCell<Store<T>>,
    data: BTreeMap<String, Option<(Rev, T)>>,
}

impl<T> Cache<'_, T>
where
    T: Clone,
{
    pub fn new(store: &RefCell<Store<T>>) -> Cache<T> {
        Cache {
            store,
            data: BTreeMap::new(),
        }
    }

    pub fn read(&mut self, key: &str) -> Option<T> {
        if !self.data.contains_key(key) {
            let record = self.store.borrow().read(key);
            self.data.insert(key.into(), record);
        }

        if let Some(Some((_, value))) = self.data.get(key) {
            Some(value.clone())
        } else {
            None
        }
    }

    pub fn write(&mut self, key: &str, value: T) -> bool {
        let old_rev = self.get_rev(key);
        let mut store = self.store.borrow_mut();

        if let Some(new_rev) = store.write(key, old_rev, value.clone()) {
            self.data.insert(key.into(), Some((new_rev, value)));
            true
        } else {
            self.data.remove(key);
            false
        }
    }

    fn get_rev(&self, key: &str) -> Option<Rev> {
        if let Some(Some((rev, _))) = self.data.get(key) {
            Some(*rev)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_for_unknown_key() {
        let store: Store<()> = Store::new();
        assert_eq!(store.seq, 0);
        assert_eq!(store.read("x"), None);
    }

    #[test]
    fn stores_a_new_value() {
        let mut store = Store::new();
        assert_eq!(store.write("x", None, 'a'), Some(1));
        assert_eq!(store.seq, 1);
        assert_eq!(store.read("x"), Some((1, 'a')));
    }

    #[test]
    fn does_not_update_a_value_without_a_rev() {
        let mut store = Store::new();
        store.write("x", None, 'a');

        assert_eq!(store.write("x", None, 'b'), None);
        assert_eq!(store.seq, 1);
        assert_eq!(store.read("x"), Some((1, 'a')));
    }

    #[test]
    fn does_not_update_a_value_with_a_bad_rev() {
        let mut store = Store::new();
        let rev = store.write("x", None, 'a').unwrap();

        assert_eq!(store.write("x", Some(rev + 1), 'b'), None);
        assert_eq!(store.seq, 1);
        assert_eq!(store.read("x"), Some((1, 'a')));
    }

    #[test]
    fn updates_a_value_with_a_matching_rev() {
        let mut store = Store::new();
        let rev = store.write("x", None, 'a').unwrap();

        assert_eq!(store.write("x", Some(rev), 'b'), Some(2));
        assert_eq!(store.seq, 2);
        assert_eq!(store.read("x"), Some((2, 'b')));
    }

    #[test]
    fn returns_all_the_keys_in_the_store() {
        let mut store = Store::new();

        store.write("/", None, 'a');
        store.write("/path/", None, 'b');
        store.write("/z/doc.json", None, 'c');

        assert_eq!(store.keys(), vec!["/", "/path/", "/z/doc.json"]);
    }

    #[test]
    fn returns_none_for_an_unknown_key() {
        let store: RefCell<Store<()>> = RefCell::new(Store::new());
        let mut cache = Cache::new(&store);

        assert_eq!(cache.read("x"), None);
    }

    #[test]
    fn reads_a_value_from_the_store() {
        let store = RefCell::new(Store::new());
        let mut cache = Cache::new(&store);

        assert_eq!(store.borrow_mut().write("x", None, 'a'), Some(1));
        assert_eq!(cache.read("x"), Some('a'));
    }

    #[test]
    fn caches_a_read_that_returns_none() {
        let store = RefCell::new(Store::new());
        let mut cache = Cache::new(&store);

        assert_eq!(cache.read("x"), None);
        assert_eq!(store.borrow_mut().write("x", None, 'a'), Some(1));
        assert_eq!(cache.read("x"), None);
    }

    #[test]
    fn writes_a_value_to_the_store() {
        let store = RefCell::new(Store::new());
        let mut cache = Cache::new(&store);

        assert_eq!(cache.write("x", 'a'), true);

        assert_eq!(store.borrow().read("x"), Some((1, 'a')));
        assert_eq!(cache.read("x"), Some('a'));
    }

    #[test]
    fn updates_a_value_in_the_store() {
        let store = RefCell::new(Store::new());
        let mut cache = Cache::new(&store);

        assert_eq!(cache.write("x", 'a'), true);
        assert_eq!(cache.write("x", 'b'), true);
        assert_eq!(cache.write("x", 'c'), true);

        assert_eq!(store.borrow().read("x"), Some((3, 'c')));
        assert_eq!(cache.read("x"), Some('c'));
    }

    #[test]
    fn fails_to_update_a_doc_it_did_not_read_first() {
        let store = RefCell::new(Store::new());
        let mut cache = Cache::new(&store);

        assert_eq!(store.borrow_mut().write("x", None, 'a'), Some(1));
        assert_eq!(cache.write("x", 'b'), false);

        assert_eq!(store.borrow().read("x"), Some((1, 'a')));
        assert_eq!(cache.read("x"), Some('a'));
    }

    #[test]
    fn fails_to_update_with_a_stale_read() {
        let store = RefCell::new(Store::new());
        let mut cache = Cache::new(&store);

        assert_eq!(cache.write("x", 'a'), true);

        assert_eq!(store.borrow_mut().write("x", Some(1), 'c'), Some(2));
        assert_eq!(cache.write("x", 'b'), false);

        assert_eq!(store.borrow().read("x"), Some((2, 'c')));
    }

    #[test]
    fn recovers_after_a_failed_write() {
        let store = RefCell::new(Store::new());
        let mut cache = Cache::new(&store);

        assert_eq!(cache.write("x", 'a'), true);

        assert_eq!(store.borrow_mut().write("x", Some(1), 'c'), Some(2));
        assert_eq!(cache.write("x", 'b'), false);

        assert_eq!(cache.read("x"), Some('c'));
        assert_eq!(cache.write("x", 'b'), true);

        assert_eq!(store.borrow().read("x"), Some((3, 'b')));
        assert_eq!(cache.read("x"), Some('b'));
    }
}
