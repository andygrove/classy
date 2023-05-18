# Classy: A Rust Library for Parsing Java Class Files

## Library

Classy is a Rust library for reading Java class files, based on The Java Virtual Machine Specification, Java SE 17 Edition.

https://docs.oracle.com/javase/specs/jvms/se17/html/jvms-4.html

### Example Usage:

```rust
let f = File::open("MyClass.class")?;
let class: ClassFile = read_class(f)?;
println!("Class JVM version: {}.{}", class.major_version, class.minor_version);
println!("Class has {} fields and {} methods", class.field_info.len(), class.method_info.len());
```

The `ClassFile` struct contains complete information about a class file:

```rust
pub struct ClassFile {
    minor_version: u16,
    major_version: u16,
    constant_pool: Vec<Constant>,
    access_flags: u16,
    this_class: u16,
    super_class: u16,
    interfaces: Vec<u16>,
    field_info: Vec<FieldInfo>,
    method_info: Vec<MethodInfo>,
    attributes: Vec<Attribute>,
}
```

## Command-Line

A command-line tool is included, which provides examples of using the library.

Features:

- Search for a class in a directory full of Jar files using a regular expression.
- Partially decompile all classes in a Jar file (similar to `javap` functionality). Note that this is not a
  full decompiler. It only shows field and method signatures, not implementation code.

## Roadmap

I built this for personal use to help me with a specific task. I am not sure if I will develop it further.
