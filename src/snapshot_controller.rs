
pub struct SnapshotController {
    cache: Vec<Snapshot>,
    current_snapshot: usize
}

#[derive(Clone)]
pub struct Snapshot {
    pub data: String,
    pub cursor_pos: (u16, u16)
}

impl SnapshotController {

    pub fn new(data: String, cursor_pos: (u16, u16)) -> SnapshotController {
        SnapshotController {
            cache: vec!(Snapshot{ data, cursor_pos }),
            current_snapshot: 0
        }
    }

    // Will clear everything above the current_state pointer to push on top of it.
    pub fn push_snapshot(&mut self, data: String, cursor_pos: (u16, u16)) {

        self.cache.drain(self.current_snapshot +1..self.cache.len());
        self.cache.push(Snapshot{ data, cursor_pos });
        self.current_snapshot += 1;
    }

    pub fn move_pointer(&mut self, num_states: isize) {
        self.current_snapshot = self.current_snapshot.saturating_add_signed(num_states);
        self.current_snapshot = self.current_snapshot.clamp(0, self.cache.len() - 1);
    }

    pub fn get_current_snapshot(&self) -> Snapshot {
        self.cache[self.current_snapshot].clone()
    }
}