use anyhow::{bail, Result};
use kwaai_provenance_eval::{read_json, run, write_new, Manifest};
use std::path::Path;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd,root,out] if cmd=="inventory"=>write_new(out,&Manifest::inventory(Path::new(root))?),
        [cmd,root,manifest] if cmd=="check"=>read_json::<Manifest>(manifest)?.check(Path::new(root)),
        [cmd,manifest,seed,count] if cmd=="sample"=>{println!("{}",serde_json::to_string_pretty(&read_json::<Manifest>(manifest)?.sample(seed.parse()?,count.parse()?)?)?);Ok(())},
        [cmd,config] if cmd=="run"=>run(read_json(config)?,false),
        [cmd,config,flag] if cmd=="run" && flag=="--resume"=>run(read_json(config)?,true),
        _=>bail!("usage: kwaai-provenance-eval inventory ROOT NEW_MANIFEST | check ROOT MANIFEST | sample MANIFEST SEED COUNT | run CONFIG [--resume]"),
    }
}
