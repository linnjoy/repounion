//! 有上限的并行映射。状态查询和 Fetch 都用它，避免一次拉起过多 git 进程。

use std::sync::Mutex;
use std::thread;

pub fn map_parallel<T, R, F>(items: Vec<T>, limit: usize, f: F) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Sync,
{
    let count = items.len();
    if count == 0 {
        return Vec::new();
    }
    let limit = limit.clamp(1, count);
    let queue = Mutex::new(items.into_iter().enumerate().collect::<Vec<_>>());
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..count).map(|_| None).collect());

    thread::scope(|scope| {
        for _ in 0..limit {
            scope.spawn(|| loop {
                let next = {
                    let mut guard = queue.lock().expect("队列锁已损坏");
                    guard.pop()
                };
                let Some((index, item)) = next else {
                    break;
                };
                let value = f(item);
                results.lock().expect("结果锁已损坏")[index] = Some(value);
            });
        }
    });

    results
        .into_inner()
        .expect("结果锁已损坏")
        .into_iter()
        .map(|item| item.expect("并行任务缺少结果"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn preserves_order() {
        let items: Vec<u32> = (0..20).collect();
        let out = map_parallel(items, 8, |n| n * 2);
        assert_eq!(out, (0..20).map(|n| n * 2).collect::<Vec<_>>());
    }

    #[test]
    fn respects_concurrency_limit() {
        let current = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let items: Vec<u32> = (0..16).collect();
        map_parallel(items, 8, |_| {
            let now = current.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(40));
            current.fetch_sub(1, Ordering::SeqCst);
        });
        let highest = peak.load(Ordering::SeqCst);
        assert!(highest > 1, "并发没有跑起来");
        assert!(highest <= 8, "并发度 {highest} 超过上限");
    }
}
