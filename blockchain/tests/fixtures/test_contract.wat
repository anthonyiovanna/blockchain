(module
  ;; Import host functions
  (import "env" "gas" (func $gas (param i32)))
  (import "env" "storage_read" (func $storage_read (param i32 i32) (result i32)))
  (import "env" "storage_write" (func $storage_write (param i32 i32 i32 i32)))
  (import "env" "emit_event" (func $emit_event (param i32 i32 i32 i32)))

  ;; Memory declaration
  (memory (export "memory") 1)

  ;; Data section for strings
  (data (i32.const 0) "test_key")
  (data (i32.const 16) "test_value")
  (data (i32.const 32) "TestEvent")

  ;; Test method that returns 42
  (func (export "test") (result i32)
    (call $gas (i32.const 10))
    i32.const 42)

  ;; Store value in state
  (func (export "store_value") (param i32 i32)
    ;; Charge gas for storage operation
    (call $gas (i32.const 100))
    
    ;; Write to storage
    (call $storage_write
      (i32.const 0)     ;; key ptr ("test_key")
      (i32.const 8)     ;; key length
      (i32.const 16)    ;; value ptr ("test_value")
      (i32.const 10))   ;; value length
    
    ;; Emit event
    (call $emit_event
      (i32.const 32)    ;; event name ptr ("TestEvent")
      (i32.const 9)     ;; event name length
      (i32.const 16)    ;; data ptr ("test_value")
      (i32.const 10)))  ;; data length

  ;; Read value from state
  (func (export "read_value") (result i32)
    ;; Charge gas for read operation
    (call $gas (i32.const 50))
    
    ;; Read from storage
    (call $storage_read
      (i32.const 0)     ;; key ptr ("test_key")
      (i32.const 8)))   ;; key length
)
