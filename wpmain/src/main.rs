use std::fmt::Display;

struct User {
    username: String,
    email: String,
    sign_in_count: u64,
    active: bool,
}

fn s() {
    //  声明一个不可变的结构体
    let user1 = User {
        email: String::from("someone@example.com"),
        username: String::from("someone"),
        active: true,
        sign_in_count: 1,
    };
    println!("{}", user1.active);

    // 声明一个可变的结构体
    let mut user2 = User {
        email: String::from("someone@example.com"),
        username: String::from("someone"),
        active: true,
        sign_in_count: 1,
    };
    user2.email = String::from("someone@qq.com");
    println!("{}", user2.email);

    let user3 = User {
        email: String::from("some1@qq.com"),
        username: String::from("somename"),
        active: user1.active,
        sign_in_count: user1.sign_in_count,
    };

    //.. 语法指定了剩余未显式设置值的字段应有与给定实例对应字段相同的值。
    let use4 = User {
        email: String::from("email"),
        ..user1
    };
}

fn main() {
    s();
    // let string1 = String::

    let string1 = String::from("abcd");
    let result = "";
    {
        {
            let string2 = String::from("efg");
            let result = longest(&string1, &string2);
            println!("the longest string is {}", result);
        }
    }
    println!("==={}", result);
    // let result = println!("Hello, world!");
    #[warn(unused_doc_comments)]
    /// 静态生命周期
    /// 这里有一种特殊的生命周期值得讨论：'static，其生命周期能够存活于整个程序期间。
    /// 所有的字符串字面值都拥有 'static 生命周期，我们也可以选择像下面这样标注出来：
    let _s: &'static str = "I have a static lifetime.";
    main2()
}

// 返回值的生命周期范围是2个参数作用域最小的那个参数的生命周期作用域
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() {
        x
    } else {
        y
    }
}

struct ImportantExcerpt<'a> {
    part: &'a str,
}

impl<'a> ImportantExcerpt<'a> {
    fn level(&self) -> i32 {
        21
    }

    fn announce_and_return_part(&self, announcement: &str) -> &str {
        println!("Attention please: {}", announcement);
        self.part
    }
}

fn longest_with_an_announcement<'a, T>(x: &'a str, y: &'a str, ann: T) -> &'a str
where
    T: Display,
{
    println!("Announcement!{}", ann);
    if x.len() > y.len() {
        x
    } else {
        y
    }
}

struct Point<T> {
    x: T,
    y: T,
}

impl<T> Point<T> {
    pub fn x(&self) -> &T {
        &self.x
    }
}

impl Point<i32> {
    fn y(&self) -> i32 {
        self.y
    }
}

fn f() {
    let p: Point<i32> = Point { x: 1, y: 5 };
    println!("{}", p.x())
}

trait Zoom {
    fn run(self) -> String;
    fn say(self) -> String;

    fn new() -> Self;
}

struct Dog {
    name: String,
    age: u8,
}

fn no<T: Zoom>(i: T) {
    println!("{}", i.run())
}

fn no3<T: Zoom + Display>(i: T) {
    println!("{}", i.run())
}

fn no1(i: impl Zoom) {
    println!("{}", i.run())
}

fn no6() -> impl Zoom {
    Dog::new()
}

fn no2(i: impl Zoom + Display) {
    println!("{}", i.run())
}

fn no4<T, U>(i: T, _u: U) -> T
where
    T: Zoom + Display,
    U: Zoom + Display,
{
    i
}

impl Zoom for Dog {
    fn run(self) -> String {
        format!("")
    }
    fn say(self) -> String {
        format!("")
    }
    fn new() -> Dog {
        Dog {
            name: "".to_string(),
            age: 2,
        }
    }
}

fn main2() {
    let s1 = String::from("abc");
    println!("{}", calculate_length(&s1));
    println!("{}", s1);
    main3();
}

fn main3() {
    let mut s1 = String::from("");

    /// 可以使用大括号来创建一个新的作用域，以允许拥有多个可变引用，只是不能 同时 拥有
    {
        let r1 = &mut s1;
        r1.push_str("xxx");
        println!("{}", r1);
    }
    let r2 = &mut s1;
    println!("{}", r2);
    main4();
}

fn calculate_length(s: &String) -> usize {
    s.len()
}

fn main4() {
    let _s = dance();
}

// 直接返回s的所有权
fn dance() -> String {
    let s = String::from("xx");
    s
}

// 会导致悬垂指针
// fn dance() -> &String {
//     let s = String::from("xx");
//     &s
// }

fn first_word2(s: &String) -> &str {
    let bytes = s.as_bytes();
    for (i, &item) in bytes.iter().enumerate() {
        if item == b' ' {
            // return &s[0..i];
            return &s[..i];
        }
    }
    s.as_str()
}

fn first_word(s: &String) -> usize {
    let bytes = s.as_bytes();
    for (i, &item) in bytes.iter().enumerate() {
        if item == b' ' {
            return i;
        }
    }
    s.len()
}

fn first_word3<'a>(s: &'a str, _b: &str) -> &'a str {
    first_word2(&String::from(s))
}
