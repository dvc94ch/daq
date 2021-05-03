use std::any::Any;
use std::io::{Result, Write};
use std::marker::PhantomData;
use vcd::{IdCode, SimulationCommand, TimescaleUnit, Value, VarType};

pub struct Timescale {
    pub scale: u32,
    pub unit: TimescaleUnit,
}

impl Timescale {
    pub fn us(scale: u32) -> Self {
        Self {
            scale,
            unit: TimescaleUnit::US,
        }
    }
}

pub struct Variable<T> {
    _marker: PhantomData<T>,
    code: IdCode,
}

impl<T> Variable<T> {
    fn new(code: IdCode) -> Self {
        Self {
            _marker: Default::default(),
            code,
        }
    }
}

pub struct Header<W: Write> {
    writer: vcd::Writer<W>,
}

impl<W: Write> Header<W> {
    pub fn new(w: W, timescale: Timescale) -> Result<Self> {
        let mut writer = vcd::Writer::new(w);
        writer.timescale(timescale.scale, timescale.unit)?;
        Ok(Self { writer })
    }

    pub fn start_module(&mut self, name: &str) -> Result<()> {
        self.writer.add_module(name)
    }

    pub fn end_module(&mut self) -> Result<()> {
        self.writer.upscope()
    }

    pub fn add_analog(&mut self, name: &str) -> Result<Variable<f64>> {
        Ok(Variable::new(self.writer.add_var(
            VarType::Real,
            1,
            name,
            None,
        )?))
    }

    pub fn add_digital(&mut self, name: &str) -> Result<Variable<Value>> {
        Ok(Variable::new(self.writer.add_var(
            VarType::Wire,
            1,
            name,
            None,
        )?))
    }

    pub fn add_vector(&mut self, name: &str, width: u32) -> Result<Variable<Vec<Value>>> {
        Ok(Variable::new(self.writer.add_var(
            VarType::Wire,
            width,
            name,
            None,
        )?))
    }

    pub fn add_protocol(&mut self, name: &str) -> Result<Variable<String>> {
        Ok(Variable::new(self.writer.add_var(
            VarType::String,
            1,
            name,
            None,
        )?))
    }

    pub fn finish(mut self) -> Result<DumpVars<W>> {
        self.writer.enddefinitions()?;
        self.writer.begin(SimulationCommand::Dumpvars)?;
        Ok(DumpVars {
            writer: self.writer,
        })
    }
}

pub struct DumpVars<W: Write> {
    writer: vcd::Writer<W>,
}

impl<W: Write> DumpVars<W> {
    pub fn timestamp(&mut self, t: u64) -> Result<()> {
        self.writer.timestamp(t)
    }

    pub fn change_value<T: Any>(&mut self, var: &Variable<T>, val: &T) -> Result<()> {
        let val = val as &dyn Any;
        if let Some(val) = val.downcast_ref::<f64>() {
            self.writer.change_real(var.code, *val)
        } else if let Some(val) = val.downcast_ref::<Value>() {
            self.writer.change_scalar(var.code, *val)
        } else if let Some(val) = val.downcast_ref::<Vec<Value>>() {
            self.writer.change_vector(var.code, val)
        } else if let Some(val) = val.downcast_ref::<String>() {
            self.writer.change_string(var.code, val)
        } else {
            unreachable!()
        }
    }

    pub fn finish(mut self) -> Result<()> {
        self.writer.end()
    }
}

fn main() -> Result<()> {
    let mut buf = vec![];
    let mut header = Header::new(&mut buf, Timescale::us(100))?;
    header.start_module("top")?;
    let sine = header.add_analog("sine")?;
    let clock = header.add_digital("clock")?;
    let vector = header.add_vector("vector", 2)?;
    let i2c = header.add_protocol("i2c")?;
    header.end_module()?;
    let mut dump = header.finish()?;

    for i in 0..100 {
        dump.timestamp(i)?;
        dump.change_value(&sine, &(((i as f64).sin() + 1.0) * 128.0))?;
        dump.change_value(&clock, &(i % 2 == 0).into())?;
        dump.change_value(&vector, &vec![(i % 2 == 0).into(), (i % 4 == 0).into()])?;
        dump.change_value(
            &i2c,
            &if i % 2 == 0 {
                "write".into()
            } else {
                "read".into()
            },
        )?;
    }

    let s = std::str::from_utf8(&buf).unwrap();
    println!("{}", s);

    Ok(())
}
