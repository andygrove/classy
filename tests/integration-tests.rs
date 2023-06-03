mod integration_tests {
    use classy::read_class;
    use std::io::{BufReader, Result};

    #[test]
    fn parse_jre_runtime_jar() -> Result<()> {
        let java_home = std::env::var("JAVA_HOME").unwrap();
        let path = format!("{java_home}/jre/lib/rt.jar");
        let file = std::fs::File::open(&path).unwrap();
        let reader = BufReader::new(file);
        let mut zip = zip::ZipArchive::new(reader)?;
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            if file.name().ends_with(".class") {
                let _ = read_class(&mut file)?;
            }
        }
        Ok(())
    }
}
