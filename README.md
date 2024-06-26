# Rue Lang

Rue is a typed programming language which gets compiled to [CLVM](https://chialisp.com/clvm) bytecode. It's designed to be an alternative to [Chialisp](https://chialisp.com) for writing on-chain code for the [Chia blockchain](https://chia.net).

This is a very experimental compiler, and should not be used in production at this time. Feel free to try the examples though!

## Setup

First, [install Rust](https://rustup.rs/) and clone this repository.
Then from the repository root, run the following command to install the CLI:

```bash
cargo install --path crates/rue-cli
```

Now you can run the examples, such as:

```bash
rue hello_world.rue
rue factorial.rue
```

You cannot currently pass arguments to the programs, however you can also run with [chia-dev-tools](https://github.com/Chia-Network/chia-dev-tools):

```bash
brun -x ff02ffff01ff018d48656c6c6f2c20776f726c6421ff0180 80
```

## Compilation

There are a series of compiler passes used to construct the final CLVM output:

### Source

Currently, a single file is used as the source for a Rue program. It's read into memory as a UTF-8 encoded string.

### Lexer

The source text is then split into tokens by the lexer. Each token represents things such as punctuation, strings, identifiers, and keywords. This is done to improve performance and enhance error messages during the parser phase.

### Parser

The hand written recursive descent parser is responsible for implementing the language's grammar. You begin top down with the whole program, then for example parse a series of functions, where each function has a parameter list, and so on. Ultimately you end up with a Concrete Syntax Tree (CST) containing your entire program broken up into meaningful segments.

### AST

The CST is not ideal for processing by the compiler since it's untyped and contains tokens it doesn't care about, such as whitespace and keywords. So in this phase, the CST is transformed into an Abstract Syntax Tree (AST), which is a strongly typed representation of all of the parts of the syntax we care about in the compiler and adjacent tooling. However, you can still at any time take an AST node and get its underlying CST node for things such as error reporting.

### HIR

The AST gets transformed into the HIR (high-level intermediate representation) in a couple passes. First, the symbol table is populated with function declarations. This allows you to call functions which have been defined after the code you're evaluating. Next, the functions are actually themselves evaluated, converting AST expressions into HIR nodes. Type checking, name resolution, and error reporting are done during this phase.

### LIR

Once the typed HIR has been built, it is then translated to a much simpler form with all language constructs boiled down to their CLVM counterparts. This is the low-level intermediate representation. Optimizations are applied during this phase, including tree shaking (removing dead code) and expression simplification.

### Codegen

Finally, you can generate CLVM from the LIR through a series of transformations and some additional optimizations can be applied at the end.
