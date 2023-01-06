iftpfm2
=======

iftpfm2 is a command line program that transfers files from one FTP server to another, based on a configuration file.

This program is written largely by ChatGPT. I don't know Rust at all. Almost everything is the result of asking ChatGPT to implement required features.

Installation
============

Installation

To install the ifptfm2 program, follow these steps:

1. First, make sure you have Git installed on your system. You can check if Git is already installed by running the following command in your terminal:

    git --version

    If Git is not installed, you can install it by following the instructions on the Git website.  

2. Next, clone the ifptfm2 repository by running the following command:

    git clone https://github.com/<your_username>/ifptfm2.git

    This will create a new directory called ifptfm2 in your current location, containing all the source code for the program.

3. Change into the ifptfm2 directory by running:

    cd ifptfm2

4. Finally, build the program by running:

    cargo build --release

This will compile the program and create an executable file called ifptfm2 in the target/release directory.

You can then run the program by typing ./target/release/ifptfm2 followed by the appropriate command line arguments (e.g. ./target/release/ifptfm2 config_file.txt).



Usage
=====

To use iftpfm2, you need to create a configuration file that specifies the connection details for the FTP servers, and the files to be transferred. The configuration file should have the following format:

# This is a comment
ip_address_from,port_from,login_from,password_from,path_from,ip_address_to,port_to,login_to,password_to,path_to,age

    ip_address_from is the IP address of the FTP server to transfer files from.
    port_from is the port number of the FTP server to transfer files from.
    login_from is the login name to use to connect to the FTP server to transfer files from.
    password_from is the password to use to connect to the FTP server to transfer files from.
    path_from is the path on the FTP server to transfer files from. You can use * as a wildcard to transfer all files in the directory.
    ip_address_to is the IP address of the FTP server to transfer files to.
    port_to is the port number of the FTP server to transfer files to.
    login_to is the login name to use to connect to the FTP server to transfer files to.
    password_to is the password to use to connect to the FTP server to transfer files to.
    path_to is the path on the FTP server to transfer files to.
    age is the minimum age of the files to be transferred, in seconds.

Once you have created the configuration file, you can run iftpfm2 with the following command:

iftpfm2 config_file

You can also use the following options:

    -h: Print usage information and exit.
    -v: Print version information and exit.
    -d: Delete the source files after transferring them.
    -l logfile: Write log information to the specified log file.

Example
=======

Here is an example configuration file that transfers all files in the /incoming directory on the FTP server at 192.168.0.1 to the /outgoing directory on the FTP server at 192.168.0.2, if they are at least one day old:

~~~
192.168.0.1,21,user1,password1,/incoming/*,192.168.0.2,21,user2,password2,/outgoing/,86400
~~~

License
=======

iftpfm2 is distributed under the terms of the MIT license. See LICENSE for details.
