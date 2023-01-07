#!/usr/bin/env bash

mkdir /tmp/ftp1
mkdir /tmp/ftp2

echo Start first FTP server on port 2121
python3 -m pyftpdlib -p 2121 -u u1 -P p1 -d /tmp/ftp1 -w &
ftp1_pid=$!

echo Start second FTP server on port 2122
python3 -m pyftpdlib -p 2122 -u u2 -P p2 -d /tmp/ftp2 -w &
ftp2_pid=$!

echo pythnons
ps axuww|grep python
echo Generate some files in the first servers directory
echo "test1" > /tmp/ftp1/test1.txt
echo "test2" > /tmp/ftp1/test2.txt
echo "test3" > /tmp/ftp1/test3.txt

echo Create config file for iftpfm2
echo "localhost,2121,u1,p1,/,localhost,2122,u2,p2,/,1" > /tmp/config.txt

sleep 2
echo Run iftpfm2 without file pattern so it uses default ".*\.txt"
./target/debug/iftpfm2 /tmp/config.txt

echo Run iftpfm2 using the config file, the -d option to delete source files and -x to match txt files
./target/debug/iftpfm2 -d -x ".*\.txt" /tmp/config.txt

echo Ensure that the files were moved to the second servers directory and deleted from the source server

echo

if [ -f "/tmp/ftp2/test1.txt" ] && [ -f "/tmp/ftp2/test2.txt" ] && [ -f "/tmp/ftp2/test3.txt" ] && [ ! -f "/tmp/ftp1/test1.txt" ] && [ ! -f "/tmp/ftp1/test2.txt" ] && [ ! -f "/tmp/ftp1/test3.txt" ]; then
    echo "SUCCESS: files transferred and deleted as expected"
else
    echo "ERROR: unexpected file transfer or deletion"
fi

echo

echo Stop the FTP servers

kill $ftp1_pid
kill $ftp2_pid
rm -rf /tmp/ftp1
rm -rf /tmp/ftp2
rm -f /tmp/config.txt
