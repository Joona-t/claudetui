use anyhow::Result;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct PtySession {
    master_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    parser: Arc<Mutex<vt100::Parser>>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    master: Box<dyn MasterPty + Send>,
    alive: Arc<Mutex<bool>>,
    /// Bumped by the reader thread every time new PTY output is processed into
    /// the vt100 parser. The render loop compares this against the last value
    /// it saw to decide whether the terminal pane needs a real redraw —
    /// avoids taking the parser lock just to detect "nothing changed".
    generation: Arc<AtomicU64>,
}

impl PtySession {
    pub fn spawn(command: &str, directory: &str, rows: u16, cols: u16) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(command);
        cmd.cwd(directory);

        // Set TERM so Claude Code renders properly
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd)?;

        // Drop the slave — we only need the master side
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 1000)));
        let master_writer = Arc::new(Mutex::new(writer));
        let alive = Arc::new(Mutex::new(true));
        let generation = Arc::new(AtomicU64::new(0));

        // Spawn reader thread that feeds PTY output into vt100 parser
        let parser_clone = Arc::clone(&parser);
        let alive_clone = Arc::clone(&alive);
        let generation_clone = Arc::clone(&generation);
        thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        if let Ok(mut p) = parser_clone.lock() {
                            p.process(&buf[..n]);
                        }
                        // New output landed — bump the generation counter so the
                        // render loop knows this session's screen changed.
                        generation_clone.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => break,
                }
            }
            if let Ok(mut a) = alive_clone.lock() {
                *a = false;
            }
        });

        Ok(Self {
            master_writer,
            parser,
            child: Arc::new(Mutex::new(child)),
            master: pair.master,
            alive,
            generation,
        })
    }

    /// Write bytes to the PTY (keyboard input)
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.master_writer.lock()
            .map_err(|_| anyhow::anyhow!("PTY writer lock poisoned"))?;
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    /// Get the current screen contents from the vt100 parser
    pub fn screen(&self) -> vt100::Screen {
        match self.parser.lock() {
            Ok(parser) => parser.screen().clone(),
            Err(poisoned) => poisoned.into_inner().screen().clone(),
        }
    }

    /// Resize the PTY
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut parser = self.parser.lock()
            .map_err(|_| anyhow::anyhow!("PTY parser lock poisoned"))?;
        parser.set_size(rows, cols);
        Ok(())
    }

    /// Monotonically increasing counter, bumped once per chunk of PTY output
    /// processed. Cheap (single atomic load, no lock) — used by the render
    /// loop to decide whether the terminal pane actually needs to be rebuilt
    /// this frame, instead of rebuilding it unconditionally every tick.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Check if the child process is still running
    pub fn is_alive(&self) -> bool {
        if let Ok(alive) = self.alive.lock() {
            *alive
        } else {
            false
        }
    }

    /// Kill the child process
    pub fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.try_wait(); // Reap to prevent zombie
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        self.kill();
    }
}
