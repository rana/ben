# Ben - Advanced Rust Performance Benchmarking Framework

## Project Overview
Ben is a sophisticated benchmarking framework implemented in Rust that provides precise CPU cycle-level performance measurements, offering a powerful toolkit for analyzing and comparing Rust code performance.

Benchmark, query, and compare Rust performance.
- Measure function performance in CPU cycles.
- Query benchmarks with user-defined labels.
- Aggregate and compare function statistics.
- Display analysis in command-line tables.

See the [mtr](https://github.com/rana/mtr) project for example benchmarks, queries and code.

## Key Features

### Core Functionality
- Precise CPU cycle measurement using low-level x86_64 processor instructions (RDTSC/RDTSCP)
- Memory fence operations for accurate timing measurements
- Parallel benchmark execution using thread pools
- Statistical analysis including median, average, min, and max calculations
- Query-based benchmark selection and comparison
- Command-line table visualization of results

### Technical Implementation
- Custom traits and generics for flexible label management
- Zero-cost abstractions for performance measurements
- Safe abstractions over unsafe CPU instructions
- Thread-safe design with message passing
- Generic benchmark function handling with opaque function pointers
- Comprehensive error handling using anyhow

### Advanced Features
- Label-based benchmark organization and filtering
- Automated overhead calculation and compensation
- Customizable statistical analysis
- Support for manual timing control
- Built-in protection against compiler optimizations

## Technical Achievements

### Performance Optimization
- Direct CPU cycle measurement instead of system time for microsecond precision
- Parallel benchmark execution for efficient testing of large benchmark suites
- Memory fence instructions for accurate timing across CPU instruction reordering

### Architecture Design
- Query builder pattern for intuitive benchmark selection and comparison
- Type-safe label system using Rust's enum system
- Thread pool implementation for parallel benchmark execution
- Channel-based communication for benchmark results

### Safety and Reliability
- Safe abstractions over unsafe CPU instructions
- Comprehensive error handling for benchmark configuration
- Thread-safe design patterns
- Protection against compiler optimizations affecting benchmark accuracy

## Implementation Details

### Key Components
1. Study (`Stdy`): Main benchmark orchestrator
2. Registry Builder (`RegBld`): Benchmark function registration
3. Query Builder (`QryBld`): Benchmark selection and comparison
4. Statistical Analysis (`Sta`): Data processing and analysis
5. Table Visualization: Results presentation

### Technical Stack
- Pure Rust implementation
- x86_64 assembly instructions via Rust intrinsics
- Thread pools for parallel execution
- Message passing for thread communication
- Custom table formatting for result presentation

## Project Value
- Enables precise performance analysis of Rust code
- Supports data-driven optimization decisions
- Facilitates performance regression testing
- Provides insights into CPU-level code behavior

## Demonstrated Skills
- Advanced Rust programming
- Low-level system programming
- Parallel computing implementation
- Performance optimization
- Safe abstraction design
- Technical documentation

### File Tree

```sh
.
├── Cargo.lock
├── Cargo.toml
├── examples
│   └── simple.rs
├── LICENSE
├── README.md
└── src
    ├── lib.rs
    ├── prv.1.rs
    ├── prv.2.rs
    ├── tbl.rs
    └── tst.rs

3 directories, 10 files
```

