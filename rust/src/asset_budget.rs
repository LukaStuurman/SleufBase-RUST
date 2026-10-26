//! A shared encoded-asset quota, reserved before each underlying write.
use std::{
    io::{self, Write},
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) struct Budget {
    limit: u64,
    used: AtomicU64,
}
impl Budget {
    pub(crate) fn new(limit: u64) -> Self {
        Self {
            limit,
            used: AtomicU64::new(0),
        }
    }
    pub(crate) fn used(&self) -> u64 {
        self.used.load(Ordering::Relaxed)
    }
    pub(crate) fn remaining(&self) -> u64 {
        self.limit - self.used()
    }
    pub(crate) fn write<W: Write>(&self, out: &mut W, bytes: &[u8]) -> io::Result<usize> {
        let requested =
            u64::try_from(bytes.len()).map_err(|_| io::Error::other("Asset write too large"))?;
        let mut used = self.used();
        loop {
            let next = used
                .checked_add(requested)
                .filter(|next| *next <= self.limit)
                .ok_or_else(|| {
                    io::Error::other("Voorbereide assets overschrijden het exportbudget")
                })?;
            match self
                .used
                .compare_exchange_weak(used, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => break,
                Err(current) => used = current,
            }
        }
        match out.write(bytes) {
            Ok(count) if count <= bytes.len() => {
                self.used
                    .fetch_sub(requested - count as u64, Ordering::Relaxed);
                Ok(count)
            }
            Ok(_) => Err(io::Error::other(
                "Asset writer returned an invalid byte count",
            )),
            Err(error) => {
                self.used.fetch_sub(requested, Ordering::Relaxed);
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_limit_and_overflow_reject_before_io() {
        let budget = Budget::new(4);
        let mut out = Vec::new();
        assert_eq!(budget.write(&mut out, b"1234").unwrap(), 4);
        assert_eq!(budget.remaining(), 0);
        assert!(budget.write(&mut out, b"5").is_err());
        assert_eq!(out, b"1234");
        let full = Budget {
            limit: u64::MAX,
            used: AtomicU64::new(u64::MAX),
        };
        assert!(full.write(&mut out, b"x").is_err());
        assert_eq!(full.used(), u64::MAX);
    }
    struct Partial;
    impl Write for Partial {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len().min(2))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    struct Failed;
    impl Write for Failed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("failure"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn partial_and_failed_writes_refund_unwritten_bytes() {
        let budget = Budget::new(4);
        assert_eq!(budget.write(&mut Partial, b"1234").unwrap(), 2);
        assert_eq!(budget.used(), 2);
        assert!(budget.write(&mut Failed, b"12").is_err());
        assert_eq!(budget.remaining(), 2);
        assert_eq!(budget.write(&mut Vec::new(), b"12").unwrap(), 2);
    }
    #[test]
    fn reservation_is_visible_while_another_writer_is_inside_io() {
        struct Observed<'a>(&'a Budget);
        impl Write for Observed<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                assert_eq!(self.0.used(), bytes.len() as u64);
                let mut other = Vec::new();
                assert!(self.0.write(&mut other, b"x").is_err());
                assert!(other.is_empty());
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let budget = Budget::new(4);
        budget.write(&mut Observed(&budget), b"1234").unwrap();
    }
    #[test]
    fn concurrent_writers_reserve_before_io_and_never_exceed_quota() {
        let budget = Budget::new(1000);
        let total = std::thread::scope(|scope| {
            let handles = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        let mut out = Vec::new();
                        while budget.write(&mut out, &[0; 7]).is_ok() {
                            assert!(budget.used() <= 1000);
                        }
                        out.len()
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .sum::<usize>()
        });
        assert_eq!(total as u64, budget.used());
        assert_eq!(total, 994);
        assert_eq!(budget.remaining(), 6);
    }
}
