use std::convert::From;
mod fan;

#[derive(Debug)]
struct Square {
    width: i32,
    height: i32,
}
impl From<i32> for Square {
    fn from(size: i32) -> Self {
        Square {
            width: size,
            height: size,
        }
    }
}
impl Square {
    fn area(&self) -> i32 {
        self.width * self.height
    }
}

mod module_name {
    fn func1() {}
    fn func2() {}
}

#[cfg(test)]
mod tests {
    use crate::fan::*;
    #[test]
    #[ignore]
    fn into() {
        /// cargo test -- --nocapture  可以打印出 println info
        ///  rust里的类型转换（一）：From与Into trait
        use super::Square;
        let s = Square::from(10);
        println!("s is {:?},area is {}", s, s.area());
        let a: Square = 20.into();
        ///这里的Square类型需要显示标注
        println!("a is {:?}", a.area());
    }
    /// --ignored：被标记为 #[ignore] 的测试用例默认不会被执行，指定此项以运行这些测试用例（通常是预计耗时很长的测试）；
    #[test]
    fn fanxing_t() {
        // use crate::fan2;
        // fan2::fanxingf();
        // mod fan;
        // fan::fanxingf2();
    }
    #[test]
    fn fan_mod() {
        let number_list = vec![1, 2, 3, 4, 5];
        let result = largest(&number_list);
        println!("The largest number is {}", result);

        let char_list = vec!['a', 'b', 'c', 'g'];

        let char_list = largest(&char_list);

        println!("The largest char is {}", result);
    }
}
