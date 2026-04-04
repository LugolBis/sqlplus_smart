= sqlplus_next : Modern SQL*Plus CLI (Rust)

A modern, developer-friendly command-line interface built in Rust as a sleek alternative to Oracle’s SQL*Plus.

This tool enhances the classic SQL*Plus experience with usability improvements that make everyday database interactions faster and more intuitive.

= 📦 Getting started
== Install the CLI
- Using cargo :
    ```bash
    cargo install --git https://github.com/LugolBis/sqlplus_next.git
    ```
- Using Git :
    ```bash
    git clone https://github.com/LugolBis/sqlplus_next.git && cd sqlplus_next && cargo build --release
    ```

== Usage :
- SQL*Plus alone :
    ```bash
    sqlplus_next sqlplus benchsql/oracle@127.0.0.1:1521/FREEPDB1
    ```
- SQL*Plus with **Docker** :
    Start the container :
    ```bash
    docker run -d --name oracle-free -p 1521:1521 -p 5500:5500 -e ORACLE_PWD=oracle container-registry.oracle.com/database/free:23.5.0.0
    ```
    Start SQL*Plus :
    ```bash
    sqlplus_next docker exec -it oracle-free sqlplus benchsql/oracle@127.0.0.1:1521/FREEPDB1
    ```

= ✨ Features
== 📝 Line Editing

Enjoy a smooth and efficient command-line experience with full line editing support :

Navigate within commands using arrow keys
Edit queries inline before execution
Easily correct mistakes without retyping everything

== 📜 Command History

Never lose track of your previous commands :

Access command history with up/down arrows
Quickly reuse and modify past queries
Boost productivity during repetitive workflows

== 🧹 Clear Terminal

Keep your workspace clean and focused :

Instantly clear the terminal screen with a simple command
Improve readability during long sessions

== ❗ Error Highlighting

Spot issues immediately:

Error messages are highlighted in red
Makes debugging faster and more intuitive
Reduces the chance of missing critical feedback

=== 🛠️ Built With
Rust — for performance, safety, and reliability

