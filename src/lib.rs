use byteorder::{BigEndian, ReadBytesExt};
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::prelude::*;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::{fs, io};

pub fn decompile_jar(
    reader: impl Read + Seek,
    output: &Path,
    verbose: bool,
) -> zip::result::ZipResult<()> {
    let mut zip = zip::ZipArchive::new(reader)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        if file.name().ends_with(".class") {
            let class = read_class(&mut file)?;

            // TODO replace .class with .java
            let mut output_path = PathBuf::from(output.clone());
            for path in file.name().split('/') {
                output_path.push(path);
                if !path.ends_with(".class") {
                    let x = format!("{}", output_path.display());
                    fs::create_dir_all(x)?;
                }
            }
            if verbose {
                println!(
                    "Decompiling {} to {} ...",
                    file.name(),
                    output_path.display()
                );
            }
            write(&class, &output_path.to_owned())?;
        }
    }
    Ok(())
}

pub fn write(class: &ClassFile, path: &PathBuf) -> io::Result<()> {
    let mut f = File::create(path)?;

    match &class.constant_pool[class.this_class as usize - 1] {
        Constant::ClassInfo { name_index } => {
            let class_name = class.get_constant_utf8(*name_index)?;
            let pos = class_name.rfind('/').unwrap();
            writeln!(f, "package {};\n", &class_name[0..pos].replace("/", "."))?;
            writeln!(f, "class {} {{", &class_name[pos + 1..])?;
        }
        _ => return Err(io::Error::new(ErrorKind::InvalidData, "this_class corrupt")),
    }

    for m in &class.field_info {
        let field_name = class.get_constant_utf8(m.name_index)?;
        let field_type = class.get_constant_utf8(m.descriptor_index)?;
        let java_type = parse_type(field_type)?;
        writeln!(f, "  {} {};\n", java_type, field_name,)?;
    }

    for m in &class.method_info {
        // for a in &m.attributes {
        //     match a {
        //         Attribute::Signature(i) => {
        //             let sig = class.get_constant_utf8(m.name_index)?;
        //             writeln!(f, "  // Signature: {sig}")?;
        //         }
        //         _ => {}
        //     }
        // }

        let method_name = class.get_constant_utf8(m.name_index)?;
        let method_type = class.get_constant_utf8(m.descriptor_index)?;
        let sig = parse_sig(method_type)?;
        writeln!(
            f,
            "  {} {}({});\n",
            sig.return_type,
            method_name,
            sig.arguments
                .iter()
                .map(|a| format!("{}", a))
                .collect::<Vec<_>>()
                .join(", ")
        )?;
    }
    writeln!(f, "}}")
}

pub fn read_class(mut rdr: impl Read) -> io::Result<ClassFile> {
    let magic = rdr.read_u32::<BigEndian>()?;
    if magic != 0xCAFEBABE {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Invalid magic number",
        ));
    }
    let minor_version = rdr.read_u16::<BigEndian>()?;
    let major_version = rdr.read_u16::<BigEndian>()?;

    let constant_pool = read_constant_pool(&mut rdr)?;

    let access_flags = rdr.read_u16::<BigEndian>()?;
    let this_class = rdr.read_u16::<BigEndian>()?;
    let super_class = rdr.read_u16::<BigEndian>()?;

    let interfaces_count = rdr.read_u16::<BigEndian>()?;
    let mut interfaces = Vec::with_capacity(interfaces_count as usize);
    for _ in 0..interfaces_count {
        interfaces.push(rdr.read_u16::<BigEndian>()?);
    }

    // println!("Reading fields ...");
    let fields_count = rdr.read_u16::<BigEndian>()?;
    let mut field_info = Vec::with_capacity(fields_count as usize);
    for _ in 0..fields_count {
        let access_flags = rdr.read_u16::<BigEndian>()?;
        let name_index = rdr.read_u16::<BigEndian>()?;
        let descriptor_index = rdr.read_u16::<BigEndian>()?;
        let attributes = read_attributes(&mut rdr, &constant_pool)?;
        field_info.push(FieldInfo {
            access_flags,
            name_index,
            descriptor_index,
            attributes,
        });
    }

    //TODO this is mostly copy and past of FieldInfo
    //println!("Reading methods ...");
    let method_count = rdr.read_u16::<BigEndian>()?;
    let mut method_info = Vec::with_capacity(method_count as usize);
    for _ in 0..method_count {
        let access_flags = rdr.read_u16::<BigEndian>()?;
        let name_index = rdr.read_u16::<BigEndian>()?;
        let descriptor_index = rdr.read_u16::<BigEndian>()?;
        let attributes = read_attributes(&mut rdr, &constant_pool)?;
        method_info.push(MethodInfo {
            access_flags,
            name_index,
            descriptor_index,
            attributes,
        });
    }

    // println!("Reading ClassFile attributes ...");
    let attributes = read_attributes(&mut rdr, &constant_pool)?;

    // assert EOF
    assert!(rdr.read_u8().is_err());

    Ok(ClassFile {
        minor_version,
        major_version,
        constant_pool,
        access_flags,
        this_class,
        super_class,
        interfaces,
        field_info,
        method_info,
        attributes,
    })
}

fn read_constant_pool(mut rdr: impl Read) -> io::Result<Vec<Constant>> {
    let constant_pool_count = rdr.read_u16::<BigEndian>()?;
    let mut constant_pool = Vec::with_capacity(constant_pool_count as usize - 1);
    let mut index = 0;
    while index < constant_pool_count - 1 {
        let tag = rdr.read_u8()?;
        // println!("tag[{index} of {constant_pool_count}] = {tag}");
        assert!(tag > 0 && tag < 21);
        let c = match tag {
            1 => {
                let length = rdr.read_u16::<BigEndian>()? as usize;
                let mut buf = vec![0; length as usize];
                rdr.read_exact(&mut buf)?;
                if let Ok(str) = String::from_utf8(buf.clone()) {
                    Ok(Constant::Utf8(str))
                } else {
                    Ok(Constant::Utf8Bytes(buf.clone()))
                }
            }
            3 => Ok(Constant::Integer(rdr.read_u32::<BigEndian>()?)),
            4 => Ok(Constant::Float(rdr.read_u32::<BigEndian>()?)),
            5 => Ok(Constant::Long(
                rdr.read_u32::<BigEndian>()?,
                rdr.read_u32::<BigEndian>()?,
            )),
            6 => Ok(Constant::Double(
                rdr.read_u32::<BigEndian>()?,
                rdr.read_u32::<BigEndian>()?,
            )),
            7 => Ok(Constant::ClassInfo {
                name_index: rdr.read_u16::<BigEndian>()?,
            }),
            8 => Ok(Constant::String {
                string_index: rdr.read_u16::<BigEndian>()?,
            }),
            9 => Ok(Constant::FieldRef {
                class_index: rdr.read_u16::<BigEndian>()?,
                name_and_type_index: rdr.read_u16::<BigEndian>()?,
            }),
            10 => Ok(Constant::MethodRef {
                class_index: rdr.read_u16::<BigEndian>()?,
                name_and_type_index: rdr.read_u16::<BigEndian>()?,
            }),
            11 => Ok(Constant::InterfaceRef {
                class_index: rdr.read_u16::<BigEndian>()?,
                name_and_type_index: rdr.read_u16::<BigEndian>()?,
            }),
            12 => Ok(Constant::NameAndType {
                name_index: rdr.read_u16::<BigEndian>()?,
                descriptor_index: rdr.read_u16::<BigEndian>()?,
            }),
            15 => Ok(Constant::MethodHandle {
                reference_kind: rdr.read_u8()?,
                reference_index: rdr.read_u16::<BigEndian>()?,
            }),
            16 => Ok(Constant::MethodType {
                descriptor_index: rdr.read_u16::<BigEndian>()?,
            }),
            17 => Ok(Constant::Dynamic {
                name_and_type_index: rdr.read_u16::<BigEndian>()?,
                bootstrap_method_attr_index: rdr.read_u16::<BigEndian>()?,
            }),
            18 => Ok(Constant::InvokeDynamic {
                name_and_type_index: rdr.read_u16::<BigEndian>()?,
                bootstrap_method_attr_index: rdr.read_u16::<BigEndian>()?,
            }),
            other => Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Invalid or unsupported constant byte: {other}"),
            )),
        }?;
        constant_pool.push(c.clone());
        index += 1;

        // All 8-byte constants take up two entries in the constant_pool table of the class file.
        // If a CONSTANT_Long_info or CONSTANT_Double_info structure is the entry at index n in
        // the constant_pool table, then the next usable entry in the table is located at
        // index n+2. The constant_pool index n+1 must be valid but is considered unusable.
        if tag == 5 || tag == 6 {
            constant_pool.push(c);
            index += 1;
        }
    }
    assert_eq!(constant_pool_count as usize - 1, constant_pool.len());
    Ok(constant_pool)
}

fn read_attributes(rdr: &mut impl Read, constant_pool: &[Constant]) -> io::Result<Vec<Attribute>> {
    let attributes_count = rdr.read_u16::<BigEndian>()?;
    // println!("Reading {attributes_count} attributes ...");
    let mut attributes = Vec::with_capacity(attributes_count as usize);
    for _ in 0..attributes_count {
        let attribute_name_index = rdr.read_u16::<BigEndian>()?;
        let attributes_length = rdr.read_u32::<BigEndian>()?;
        match &constant_pool[attribute_name_index as usize - 1] {
            Constant::Utf8(str) => {
                let attr: io::Result<Attribute> = match str.as_str() {
                    "MethodParameters" => {
                        let param_count = rdr.read_u8()?;
                        let mut vec = Vec::with_capacity(param_count as usize);
                        for _ in 0..param_count {
                            let name_index = rdr.read_u16::<BigEndian>()?;
                            let access_flags = rdr.read_u16::<BigEndian>()?;
                            vec.push((name_index, access_flags));
                        }
                        Ok(Attribute::MethodParameters(vec))
                    }
                    "Code" => {
                        let max_stack = rdr.read_u16::<BigEndian>()?;
                        let max_locals = rdr.read_u16::<BigEndian>()?;
                        let code_length = rdr.read_u32::<BigEndian>()?;
                        assert!(code_length > 0 && code_length < 65536);

                        let mut code = vec![0; code_length as usize];
                        rdr.read_exact(&mut code)?;

                        let exception_table_length = rdr.read_u16::<BigEndian>()?;
                        let mut exception_table =
                            Vec::with_capacity(exception_table_length as usize);
                        for _ in 0..exception_table_length {
                            exception_table.push(ExceptionTable {
                                start_pc: rdr.read_u16::<BigEndian>()?,
                                end_pc: rdr.read_u16::<BigEndian>()?,
                                handler_pc: rdr.read_u16::<BigEndian>()?,
                                catch_type: rdr.read_u16::<BigEndian>()?,
                            });
                        }

                        let attributes = read_attributes(rdr, &constant_pool)?;

                        Ok(Attribute::Code {
                            max_stack,
                            max_locals,
                            code,
                            exception_table,
                            attributes,
                        })
                    }
                    "ScalaSig" => {
                        // ScalaSig: [5, 0, 0]
                        let mut x = vec![0; attributes_length as usize];
                        rdr.read_exact(&mut x)?;
                        Ok(Attribute::ScalaSig(x))
                    }
                    "Signature" => {
                        let signature_index = rdr.read_u16::<BigEndian>()?;
                        Ok(Attribute::Signature(signature_index))
                    }
                    _ => {
                        /*
                        Unsupported attribute: LineNumberTable
                        Unsupported attribute: LocalVariableTable
                        Unsupported attribute: RuntimeVisibleAnnotations
                        Unsupported attribute: ScalaInlineInfo
                        Unsupported attribute: SourceFile
                        Unsupported attribute: StackMapTable
                         */
                        let mut x = vec![0; attributes_length as usize];
                        rdr.read_exact(&mut x)?;
                        //println!("Unsupported attribute: {other}; {x:?}");
                        Ok(Attribute::RawBytes(x))
                    }
                };
                attributes.push(attr?);
            }
            _ => return Err(io::Error::new(ErrorKind::InvalidData, "not utf8")),
        }
    }
    Ok(attributes)
}

/// Java Class File representation, based on https://docs.oracle.com/javase/specs/jvms/se17/html/jvms-4.html
#[derive(Debug)]
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

impl ClassFile {
    fn get_constant_utf8(&self, i: u16) -> io::Result<&str> {
        match &self.constant_pool[i as usize - 1] {
            Constant::Utf8(str) => Ok(&str),
            _ => Err(io::Error::new(ErrorKind::InvalidData, "not utf8")),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Constant {
    String {
        string_index: u16,
    },
    Utf8(String),
    Utf8Bytes(Vec<u8>),
    ClassInfo {
        name_index: u16,
    },
    NameAndType {
        name_index: u16,
        descriptor_index: u16,
    },
    FieldRef {
        class_index: u16,
        name_and_type_index: u16,
    },
    MethodRef {
        class_index: u16,
        name_and_type_index: u16,
    },
    InterfaceRef {
        class_index: u16,
        name_and_type_index: u16,
    },
    MethodType {
        descriptor_index: u16,
    },
    MethodHandle {
        reference_kind: u8,
        reference_index: u16,
    },
    Integer(u32),
    Float(u32),
    Long(u32, u32),
    Double(u32, u32),
    Dynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    InvokeDynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
}

#[derive(Debug)]
pub struct FieldInfo {
    access_flags: u16,
    name_index: u16,
    descriptor_index: u16,
    attributes: Vec<Attribute>,
}

#[derive(Debug)]
pub struct MethodInfo {
    access_flags: u16,
    name_index: u16,
    descriptor_index: u16,
    attributes: Vec<Attribute>,
}

#[derive(Debug)]
pub enum Attribute {
    /// Method parameters (name_index, access_flags)
    MethodParameters(Vec<(u16, u16)>),
    Code {
        max_stack: u16,
        max_locals: u16,
        code: Vec<u8>,
        exception_table: Vec<ExceptionTable>,
        attributes: Vec<Attribute>,
    },
    Signature(u16),
    ScalaSig(Vec<u8>),
    /// Unparsed attribute
    RawBytes(Vec<u8>),
}

#[derive(Debug)]
pub struct ExceptionTable {
    start_pc: u16,
    end_pc: u16,
    handler_pc: u16,
    catch_type: u16,
}

#[derive(Debug, PartialEq, Eq)]
pub enum JavaType {
    Void,
    Boolean,
    Byte,
    Char,
    Double,
    Float,
    Short,
    Int,
    Long,
    Class(String),
    Array(Box<JavaType>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct JavaSignature {
    arguments: Vec<JavaType>,
    return_type: JavaType,
}

impl Display for JavaType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Void => write!(f, "void"),
            Self::Boolean => write!(f, "boolean"),
            Self::Byte => write!(f, "byte"),
            Self::Char => write!(f, "char"),
            Self::Short => write!(f, "short"),
            Self::Int => write!(f, "int"),
            Self::Long => write!(f, "long"),
            Self::Double => write!(f, "double"),
            Self::Float => write!(f, "float"),
            Self::Class(c) => write!(f, "{}", c),
            Self::Array(t) => write!(f, "{}[]", t),
        }
    }
}

fn parse_type(t: &str) -> io::Result<JavaType> {
    let (a, _) = parse_type_from(t, 0)?;
    Ok(a)
}

fn parse_type_from(t: &str, i: usize) -> io::Result<(JavaType, usize)> {
    // The field descriptor of an instance variable of type int is simply I.
    // The field descriptor of an instance variable of type Object is Ljava/lang/Object;. Note that the internal form of the binary name for class Object is used.
    // The field descriptor of an instance variable of the multidimensional array type double[][][] is [[[D.
    match t.chars().nth(i).unwrap() {
        'V' => Ok((JavaType::Void, i + 1)),
        'B' => Ok((JavaType::Byte, i + 1)),
        'C' => Ok((JavaType::Char, i + 1)),
        'D' => Ok((JavaType::Double, i + 1)),
        'F' => Ok((JavaType::Float, i + 1)),
        'S' => Ok((JavaType::Short, i + 1)),
        'I' => Ok((JavaType::Int, i + 1)),
        'J' => Ok((JavaType::Long, i + 1)),
        'Z' => Ok((JavaType::Boolean, i + 1)),
        'L' => {
            let remaining = &t[i + 1..];
            let pos = remaining.find(';').unwrap();
            let class_name = remaining[0..pos].to_owned();
            Ok((JavaType::Class(class_name.replace("/", ".")), i + pos + 2))
        }
        '[' => {
            let (t, pos) = parse_type_from(t, i + 1)?;
            Ok((JavaType::Array(Box::new(t)), pos))
        }
        other => Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("invalid start of type: {}", other),
        )),
    }
}

fn parse_sig(t: &str) -> io::Result<JavaSignature> {
    let mut arguments = vec![];
    let mut processed_args = false;
    let mut i = 0;
    while i < t.chars().count() {
        match t.chars().nth(i).unwrap() {
            '(' => {
                assert_eq!(i, 0);
                i += 1;
            }
            ')' => {
                processed_args = true;
                i += 1;
            }
            _ => {
                let (java_type, offset) = parse_type_from(t, i)?;
                i = offset;
                if processed_args {
                    return Ok(JavaSignature {
                        arguments,
                        return_type: java_type,
                    });
                } else {
                    arguments.push(java_type);
                }
            }
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempdir::TempDir;

    #[test]
    fn test_parse_primitives() {
        assert_eq!("byte", format!("{}", parse_type("B").unwrap()));
        assert_eq!("char", format!("{}", parse_type("C").unwrap()));
        assert_eq!("short", format!("{}", parse_type("S").unwrap()));
        assert_eq!("int", format!("{}", parse_type("I").unwrap()));
        assert_eq!("long", format!("{}", parse_type("J").unwrap()));
        assert_eq!("float", format!("{}", parse_type("F").unwrap()));
        assert_eq!("double", format!("{}", parse_type("D").unwrap()));
    }

    #[test]
    fn test_parse_class() {
        assert_eq!(
            "java.lang.Object",
            format!("{}", parse_type("Ljava/lang/Object;.").unwrap())
        );
    }

    #[test]
    fn test_parse_array() {
        assert_eq!("double[][][]", format!("{}", parse_type("[[[D").unwrap()));
    }

    #[test]
    fn test_parse_signature() -> io::Result<()> {
        let sig = "(Ljava/lang/String;Ljava/util/Map;)Lorg/apache/spark/sql/Dataset;";
        let sig = parse_sig(sig)?;
        let expected = JavaSignature {
            arguments: vec![
                JavaType::Class("java.lang.String".to_owned()),
                JavaType::Class("java.util.Map".to_owned()),
            ],
            return_type: JavaType::Class("org.apache.spark.sql.Dataset".to_owned()),
        };
        assert_eq!(expected, sig);
        Ok(())
    }

    #[test]
    fn test_decompile_class() -> io::Result<()> {
        let f = File::open("testdata/SparkSession.class")?;
        let class: ClassFile = read_class(f)?;
        assert_eq!(52, class.major_version);
        assert_eq!(0, class.minor_version);
        println!("Class JVM version: {}.{}", class.major_version, class.minor_version);
        println!("Class has {} fields and {} methods", class.field_info.len(), class.method_info.len());
        Ok(())
    }

    #[test]
    fn test_decompile_jar() -> io::Result<()> {
        let f = File::open("testdata/test.jar")?;
        let tmp_dir = TempDir::new("classy")?;
        decompile_jar(f, tmp_dir.path(), true)?;
        let file_path = tmp_dir.path().join("SparkSession.class");
        let str = fs::read_to_string(file_path)?;
        println!("{str}");
        let expected = fs::read_to_string("testdata/SparkSession.decompiled")?;
        assert_eq!(expected.trim(), str.trim());
        Ok(())
    }
}
