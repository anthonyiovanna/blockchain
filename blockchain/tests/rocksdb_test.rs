use rocksdb::{DB, Options};
use tempfile::TempDir;

#[test]
fn test_rocksdb_basic_operations() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path();

    let mut opts = Options::default();
    opts.create_if_missing(true);
    
    // Open database
    let db = DB::open(&opts, path).unwrap();
    
    // Test write
    db.put(b"key1", b"value1").unwrap();
    
    // Test read
    let result = db.get(b"key1").unwrap().unwrap();
    assert_eq!(result.as_slice(), b"value1");
    
    // Test delete
    db.delete(b"key1").unwrap();
    assert!(db.get(b"key1").unwrap().is_none());
}
