# Memory Model Design for Lang

## Overview

This document describes a memory model that provides:
- **C-like malloc/free** for explicit memory allocation
- **Hierarchical Allocators** - main allocator creates child allocators
- **No function signature pollution** - pass allocator implicitly without new keywords
- **Explicit allocation behavior**
- **No reference counting**

---

## Core Problem: Passing Allocator Without Pollution

The user wants to explicitly pass an allocator to a function, but WITHOUT:
- Adding new keywords
- Polluting the function signature

### Solution: Allocator as Method Receiver

Instead of passing allocator as a parameter, make it the **method receiver**:

```lang
// Instead of: fn process(data: *i32, allocator: &Allocator)
// Use method syntax:

struct Processor {
    allocator: Allocator,

    fn new() Processor {
        Processor {
            allocator: get_main().create_child(),
        }
    }
    
    fn process(self, size: usize) *i32 {
        // Use self.allocator - no extra parameter!
        return self.allocator.alloc(size)
    }
}

// Usage
fn main() void {
    var processor = Processor.new()
    var data = processor.process(100)  // Uses processor's allocator
}
```

This approach:
- ✅ No extra parameters in function signature
- ✅ No new keywords needed
- ✅ Explicit control over which allocator is used
- ✅ Type-safe: compiler ensures correct allocator

---

## Built-in Allocator Implementations

### 1. DebugAllocator

A debugging allocator that tracks all allocations for leak detection.

**Behavior:**
- On `alloc`: prints size and address
- On `free`: prints address, marks block as reusable
- On `deinit`: panics if any unfreed blocks exist, lists all addresses and sizes

```lang
struct BlockInfo {
    size: usize,
    id: usize,
    state: BlockState,
    alloc_site: SourceLocation,  // File:line of allocation
}

enum BlockState {
    Allocated,
    Freed,  // Can be reused
}

struct DebugAllocator {
    // Internal tracking
    blocks: Map<*void, BlockInfo>,
    next_id: usize,

    /// Create new debug allocator
    fn new() DebugAllocator {
        DebugAllocator {
            blocks: Map.new(),
            next_id: 0,
        }
    }
    
    /// Allocate memory (with debug output)
    fn alloc<T>(self, size: usize) *T {
        // Get current source location
        var location = get_caller_location()
        
        // Allocate via system
        var ptr = system_alloc(size)
        
        // Track block
        var id = self.next_id
        self.next_id = self.next_id + 1
        
        self.blocks.set(ptr, BlockInfo {
            size: size,
            id: id,
            state: BlockState.Allocated,
            alloc_site: location,
        })
        
        // Debug output
        println("[DebugAlloc #{id}] ALLOC size={size} ptr={ptr}")
        
        return ptr as *T
    }
    
    /// Free memory (with debug output)
    fn free<T>(self, ptr: *T) {
        if ptr == null {
            return
        }
        
        var void_ptr = ptr as *void
        
        // Check if block exists
        if !self.blocks.contains(void_ptr) {
            println("[DebugAlloc] ERROR: Freeing unknown pointer: {void_ptr}")
            panic("Double-free or invalid pointer")
        }
        
        var info = self.blocks.get(void_ptr)
        
        if info.state == BlockState.Freed {
            println("[DebugAlloc] ERROR: Double-free: {void_ptr}")
            panic("Double-free detected")
        }
        
        // Mark as freed
        info.state = BlockState.Freed
        self.blocks.set(void_ptr, info)
        
        // Debug output
        println("[DebugAlloc #{info.id}] FREE ptr={void_ptr}")
    }
    
    /// Destroy allocator (panics if leaks detected)
    fn deinit(self) {
        // Find all unfreed blocks
        var leaks: Vec<LeakInfo> = []
        
        for (ptr, info) in self.blocks {
            if info.state == BlockState.Allocated {
                leaks.push(LeakInfo {
                    ptr: ptr,
                    size: info.size,
                    id: info.id,
                    location: info.alloc_site,
                })
            }
        }
        
        // Report leaks
        if leaks.length > 0 {
            println("[DebugAlloc] PANIC: Memory leaks detected!")
            println("==========================================")
            
            for leak in leaks {
                println("  Leak #{leak.id}:")
                println("    Address: {leak.ptr}")
                println("    Size: {leak.size} bytes")
                println("    Allocated at: {leak.location}")
            }
            
            println("==========================================")
            println("Total leaks: {leaks.length}")
            
            panic("DebugAllocator: {leaks.length} unfreed memory block(s)")
        }
        
        println("[DebugAlloc] All memory freed successfully")
    }
    
    /// Check for leaks without panicking
    fn check_leaks(self) -> bool {
        for (_, info) in self.blocks {
            if info.state == BlockState.Allocated {
                return true  // Has leaks
            }
        }
        return false
    }
}
```

**Usage Example:**

```lang
fn main() void {
    var debug = DebugAllocator.new()
    
    // Allocate
    var a = debug.alloc<i32>(10)   // Prints: [DebugAlloc #0] ALLOC size=40 ptr=0x...
    var b = debug.alloc<i32>(20)   // Prints: [DebugAlloc #1] ALLOC size=80 ptr=0x...
    
    // Free one
    debug.free(a)                   // Prints: [DebugAlloc #0] FREE ptr=0x...
    
    // Forgot to free b - will panic on deinit
    debug.deinit()                  // PANIC: Memory leaks detected!
}
```

---

### 2. ArenaAllocator

A fast allocator that pre-allocates a large chunk and manages it internally.

**Behavior:**
- On `init`: allocates one large chunk from system
- On `alloc`: bumps pointer within the chunk (very fast)
- On `free`: NO OP (marks as reusable internally)
- On `deinit`: frees entire chunk at once

```lang
struct ArenaAllocator {
    /// The large memory chunk
    buffer: *void,
    /// Total buffer size
    capacity: usize,
    /// Current allocation offset
    offset: usize,
    /// Size of all allocations (for tracking)
    total_allocated: usize,

    /// Create arena with specified capacity
    fn with_capacity(capacity: usize) ArenaAllocator {
        var buffer = system_alloc(capacity)
        
        return ArenaAllocator {
            buffer: buffer,
            capacity: capacity,
            offset: 0,
            total_allocated: 0,
        }
    }
    
    /// Create arena with default capacity (e.g., 1MB)
    fn new() ArenaAllocator {
        return ArenaAllocator.with_capacity(1024 * 1024)  // 1MB default
    }
    
    /// Allocate from arena (bump allocation)
    fn alloc<T>(self, size: usize) *T {
        // Align to next 8-byte boundary
        var aligned_size = (size + 7) & ~7
        
        // Check if we have space
        if self.offset + aligned_size > self.capacity {
            panic("ArenaAllocator: out of memory")
        }
        
        // Get pointer at current offset
        var ptr = (self.buffer as *u8).add(self.offset) as *void
        
        // Bump offset
        self.offset = self.offset + aligned_size
        self.total_allocated = self.total_allocated + aligned_size
        
        return ptr as *T
    }
    
    /// Free - NO OP in arena!
    /// 
    /// Memory is not actually freed, just marked as allocated.
    /// The entire arena will be freed when deinit is called.
    fn free<T>(self, ptr: *T) {
        // NO OP - arena doesn't support individual frees
        // This is intentional for performance
    }
    
    /// Reset arena - clear all allocations
    /// 
    /// Note: This does NOT free the underlying buffer,
    /// it just resets the offset to 0, making all
    /// previous allocations available again.
    fn reset(self) {
        self.offset = 0
        // Note: old data is still there, but will be overwritten
    }
    
    /// Get current usage
    fn used(self) -> usize {
        return self.offset
    }
    
    /// Get remaining capacity
    fn remaining(self) -> usize {
        return self.capacity - self.offset
    }
    
    /// Destroy arena - free entire buffer
    fn deinit(self) {
        if self.buffer != null {
            system_free(self.buffer)
            self.buffer = null
        }
        
        println("[ArenaAllocator] Destroyed. Total allocated: {self.total_allocated} bytes")
    }
}
```

**Usage Example:**

```lang
fn main() void {
    // Create arena with 64KB
    var arena = ArenaAllocator.with_capacity(64 * 1024)
    
    // Allocate many small objects
    var objects: Vec<*Object> = []
    
    for i in 0..1000 {
        var obj = arena.alloc<Object>(1)
        objects.push(obj)
    }
    
    // Free calls are NO OP!
    // But we can reset to start fresh
    arena.reset()
    
    // Now we can allocate again
    var more = arena.alloc<i32>(100)
    
    // Or just destroy - frees entire 64KB at once
    arena.deinit()
}
```

---

## Comparison

| Feature | DebugAllocator | ArenaAllocator |
|---------|-----------------|----------------|
| **alloc speed** | Slow (tracks) | Fast (bump) |
| **free speed** | Slow (tracks) | NO OP |
| **Memory overhead** | High (metadata) | Low |
| **Individual free** | ✓ Yes | ✗ No |
| **Reset** | ✗ No | ✓ Yes |
| **Leak detection** | ✓ Yes | ✗ No |
| **Use case** | Debugging | Production |

---

## Choosing the Right Allocator

### Use DebugAllocator when:
- Debugging memory leaks
- Testing new code
- Finding dangling pointers
- Verifying proper cleanup

### Use ArenaAllocator when:
- Performance is critical
- Many short-lived allocations
- You don't need individual frees
- Batch allocation/deallocation pattern

### Using Both Together

```lang
fn main() void {
    // Use arena for performance
    var arena = ArenaAllocator.with_capacity(1024 * 1024)
    
    // During debugging, wrap with debug allocator
    // var debug = DebugAllocator.new()
    // Use debug instead of arena
    
    // Do work...
    var data = arena.alloc<i32>(100)
    
    // Clean up
    arena.deinit()
}
```
