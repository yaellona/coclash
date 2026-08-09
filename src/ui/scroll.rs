//! 统一滚动视图：操作日志 / mihomo 日志 / 帮助共用。
/// 模型采用"选中行"语义，`viewport` 负责把选中行映射到可见区。
#[derive(Debug)]
pub struct Scroller {
    /// 选中（底端可见）行
    pub select: usize,
    pub follow: bool,
}

impl Scroller {
    pub fn new() -> Self {
        Self {
            select: 0,
            follow: true,
        }
    }

    pub fn up(&mut self) {
        self.follow = false;
        self.select = self.select.saturating_sub(1);
    }

    pub fn down(&mut self, total: usize) {
        let max = total.saturating_sub(1);
        self.select = (self.select + 1).min(max);
        if self.select == max {
            self.follow = true;
        }
    }

    pub fn page_up(&mut self, visible: usize) {
        self.follow = false;
        self.select = self.select.saturating_sub(visible.max(1));
    }

    pub fn page_down(&mut self, total: usize, visible: usize) {
        let max = total.saturating_sub(1);
        self.select = (self.select + visible.max(1)).min(max);
        if self.select == max {
            self.follow = true;
        }
    }

    /// 把 select 收敛到合法范围；follow 模式贴底（新增行时自动跟随）
    pub fn clamp(&mut self, total: usize) {
        if total == 0 {
            self.select = 0;
            return;
        }
        let max = total - 1;
        if self.follow {
            self.select = max;
        } else {
            self.select = self.select.min(max);
        }
    }

    /// 视口 [start, end)：让选中行尽量显示在最后一行
    pub fn viewport(&self, total: usize, visible: usize) -> (usize, usize) {
        if total == 0 || visible == 0 {
            return (0, 0);
        }
        let start = self.select.saturating_add(1).saturating_sub(visible);
        let end = (start + visible).min(total);
        (start, end)
    }
}

impl Default for Scroller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_follow_bottoms_out() {
        let mut s = Scroller::new();
        s.clamp(10);
        assert_eq!(s.select, 9);
        assert!(s.follow);
    }

    #[test]
    fn test_up_leaves_follow() {
        let mut s = Scroller::new();
        s.clamp(10);
        s.up();
        assert!(!s.follow);
        assert_eq!(s.select, 8);
    }

    #[test]
    fn test_down_returns_to_follow() {
        let mut s = Scroller::new();
        s.clamp(10);
        s.up();
        s.down(10);
        assert!(s.follow);
        assert_eq!(s.select, 9);
    }

    #[test]
    fn test_empty_clamp() {
        let mut s = Scroller::new();
        s.up();
        s.clamp(0);
        assert_eq!(s.select, 0);
    }

    #[test]
    fn test_page_down_bounds() {
        let mut s = Scroller::new();
        s.clamp(3);
        assert_eq!(s.select, 2);
        s.page_down(3, 1);
        assert_eq!(s.select, 2);
        assert!(s.follow);
    }

    #[test]
    fn test_viewport_shows_selected_at_bottom() {
        let s = Scroller {
            select: 9,
            follow: true,
        };
        assert_eq!(s.viewport(10, 5), (5, 10));
        let s = Scroller {
            select: 2,
            follow: true,
        };
        assert_eq!(s.viewport(10, 5), (0, 5));
    }
}
