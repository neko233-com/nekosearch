//! 内置演示数据集：开启 `--seed-demo`（或单机无种子时自动启用）时写入索引，
//! 让单机部署开箱即可搜索各编程语言的官方网站，无需外网爬取即可验证搜索可用。
//!
//! 每条文档对应一个编程语言的官方站点，标题/正文均包含语言名与「官网/官方文档」等
//! 关键词，便于验证中英文分词与 BM25 排序。id 稳定，重复写入会原地更新，不会堆积。

use nekosearch_core::Doc;

/// 返回一组内置演示文档（编程语言官方网站）。
pub fn demo_docs() -> Vec<Doc> {
    vec![
        Doc {
            id: "lang-rust".into(),
            url: "https://www.rust-lang.org/".into(),
            title: "Rust 官方网站 rust-lang.org".into(),
            body: "Rust 官方站 rust-lang.org 提供语言文档、编译器下载与学习资源。Rust 是注重内存安全与并发安全的系统级编程语言，无垃圾回收，性能媲美 C++。nekosearch 本身即用 Rust 实现。".into(),
        },
        Doc {
            id: "lang-go".into(),
            url: "https://go.dev/".into(),
            title: "Go 官方网站 go.dev".into(),
            body: "Go 官方站 go.dev 是 Go 语言的总入口，提供文档、教程与下载。Go（Golang）由 Google 设计，语法简洁、原生支持并发（goroutine），适合云原生与后端服务。".into(),
        },
        Doc {
            id: "lang-python".into(),
            url: "https://www.python.org/".into(),
            title: "Python 官方网站 python.org".into(),
            body: "Python 官方站 python.org 提供语言文档、标准库参考与安装包下载。Python 是易学易用的高级脚本语言，广泛用于数据科学、人工智能与自动化。".into(),
        },
        Doc {
            id: "lang-javascript".into(),
            url: "https://developer.mozilla.org/zh-CN/docs/Web/JavaScript".into(),
            title: "JavaScript 官方文档 MDN".into(),
            body: "JavaScript 的权威参考是 MDN Web Docs，由 Mozilla 维护，覆盖语法、标准库与 Web API。JavaScript 是浏览器的原生脚本语言，也是 Node.js 等服务端运行时的基础。ECMAScript 是其官方标准。".into(),
        },
        Doc {
            id: "lang-typescript".into(),
            url: "https://www.typescriptlang.org/".into(),
            title: "TypeScript 官方网站 typescriptlang.org".into(),
            body: "TypeScript 官方站 typescriptlang.org 提供上手指南、Playground 与手册。TypeScript 是 JavaScript 的超集，增加了静态类型，由 Microsoft 开发，编译为 JS 运行。".into(),
        },
        Doc {
            id: "lang-java".into(),
            url: "https://www.java.com/".into(),
            title: "Java 官方网站 java.com".into(),
            body: "Java 官方站 java.com 提供 JRE/JDK 下载与入门。Java 是面向对象的跨平台语言，遵循「一次编写，到处运行」，广泛用于企业后端与安卓开发；权威文档在 Oracle 的 docs.oracle.com。".into(),
        },
        Doc {
            id: "lang-cpp".into(),
            url: "https://en.cppreference.com/w/cpp".into(),
            title: "C++ 官方参考 cppreference".into(),
            body: "C++ 的权威参考资料站是 cppreference.com，覆盖标准库与语言特性。C++ 是 C 的超集，支持面向对象与泛型编程，常用于系统软件、游戏引擎与高性能计算；标准由 ISO 维护。".into(),
        },
        Doc {
            id: "lang-c".into(),
            url: "https://en.cppreference.com/w/c".into(),
            title: "C 语言官方参考 cppreference".into(),
            body: "C 语言的权威参考资料站是 cppreference.com 的 C 部分。C 是经典的过程式系统编程语言，语法简洁、贴近硬件，是操作系统与嵌入式开发的基石；标准由 ISO 维护。".into(),
        },
        Doc {
            id: "lang-csharp".into(),
            url: "https://learn.microsoft.com/dotnet/csharp/".into(),
            title: "C# 官方文档 Microsoft Learn".into(),
            body: "C# 的官方文档在 Microsoft Learn 的 .NET C# 栏目。C# 是微软推出的现代面向对象语言，运行于 .NET 运行时，广泛用于桌面、Web 与游戏（Unity）开发。".into(),
        },
        Doc {
            id: "lang-kotlin".into(),
            url: "https://kotlinlang.org/".into(),
            title: "Kotlin 官方网站 kotlinlang.org".into(),
            body: "Kotlin 官方站 kotlinlang.org 提供语言文档、在线 playground 与下载。Kotlin 是运行在 JVM 上的现代静态语言，由 JetBrains 开发，是 Android 官方首选语言。".into(),
        },
        Doc {
            id: "lang-swift".into(),
            url: "https://www.swift.org/".into(),
            title: "Swift 官方网站 swift.org".into(),
            body: "Swift 官方站 swift.org 是开源的 Swift 语言门户，提供文档与教程。Swift 由 Apple 推出，用于 iOS/macOS 开发，也可在服务端运行，语法安全且高性能。".into(),
        },
        Doc {
            id: "lang-ruby".into(),
            url: "https://www.ruby-lang.org/".into(),
            title: "Ruby 官方网站 ruby-lang.org".into(),
            body: "Ruby 官方站 ruby-lang.org 提供文档、下载与新闻。Ruby 是注重开发乐趣的动态面向对象脚本语言，以优雅语法著称，Ruby on Rails 是其著名 Web 框架。".into(),
        },
        Doc {
            id: "lang-php".into(),
            url: "https://www.php.net/".into(),
            title: "PHP 官方网站 php.net".into(),
            body: "PHP 官方站 php.net 提供语言手册、函数参考与下载。PHP 是专为 Web 开发设计的服务端脚本语言，广泛应用于内容管理系统与后端接口。".into(),
        },
        Doc {
            id: "lang-node".into(),
            url: "https://nodejs.org/".into(),
            title: "Node.js 官方网站 nodejs.org".into(),
            body: "Node.js 官方站 nodejs.org 提供运行时下载与 API 文档。Node.js 是基于 V8 引擎的 JavaScript 服务端运行时，事件驱动、非阻塞 I/O，适合高并发网络服务。".into(),
        },
        Doc {
            id: "lang-deno".into(),
            url: "https://deno.com/".into(),
            title: "Deno 官方网站 deno.com".into(),
            body: "Deno 官方站 deno.com 是新一代 JavaScript/TypeScript 运行时，由 Node.js 原作者在 Rust 上重建，默认安全、原生支持 TypeScript，强调现代 Web 标准。".into(),
        },
        Doc {
            id: "lang-scala".into(),
            url: "https://www.scala-lang.org/".into(),
            title: "Scala 官方网站 scala-lang.org".into(),
            body: "Scala 官方站 scala-lang.org 提供文档与下载。Scala 融合面向对象与函数式编程，运行于 JVM，常用于大数据处理（如 Spark）。".into(),
        },
        Doc {
            id: "lang-haskell".into(),
            url: "https://www.haskell.org/".into(),
            title: "Haskell 官方网站 haskell.org".into(),
            body: "Haskell 官方站 haskell.org 是纯函数式编程语言 Haskell 的门户，提供编译器 GHC 与教程。Haskell 以强类型与惰性求值为特色，常用于研究与教学。".into(),
        },
        Doc {
            id: "lang-dart".into(),
            url: "https://dart.dev/".into(),
            title: "Dart 官方网站 dart.dev".into(),
            body: "Dart 官方站 dart.dev 提供语言文档与 SDK 下载。Dart 是 Google 推出的客户端优化语言，是 Flutter 跨平台 UI 框架的底层语言，可编译为原生与 JS。".into(),
        },
        Doc {
            id: "lang-elixir".into(),
            url: "https://elixir-lang.org/".into(),
            title: "Elixir 官方网站 elixir-lang.org".into(),
            body: "Elixir 官方站 elixir-lang.org 提供指南与文档。Elixir 是运行于 Erlang VM 的函数式语言，擅长高并发、分布式与容错系统，常用于实时通信后端。".into(),
        },
    ]
}
