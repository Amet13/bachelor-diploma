ddos-deflate
============
Shell script blocking DDoS attacks. Fork of [(D)DoS Deflate](http://deflate.medialayer.com/).

It works on Debian 7 and CentOS 7 (please tell me if you've tested script on other distros).

Installation
------------
```bash
su -
cd /tmp
wget https://raw.githubusercontent.com/Amet13/ddos-deflate/master/install.sh
chmod +x install.sh
./install.sh
```
Add your contact e-mail:
```bash
vim /usr/local/ddos/ddos.conf
EMAIL_TO="mail@example.com"
```
Add your ignore ip's to ignorelist:
```bash
vim /usr/local/ddos/ignore.ip.list
127.0.0.1
1.1.1.1
2.2.2.2
```
Add cronjob:
```bash
crontab -e
# run script every minute
* * * * * /usr/local/ddos/ddos.sh >/dev/null 2>&1
```
Check:
```bash
/usr/local/ddos/ddos.sh
    724 127.0.0.1
    214 2.2.2.2
     59 3.3.3.3
...
```

Testing
-------
Run ab from another computer:
```bash
user@192.168.0.100 ~ $ ab -n 200000 -c 100 http://server-ip/
```
Check new IPTables rules on server:
```bash
iptables -L INPUT
Chain INPUT (policy ACCEPT)
target     prot opt source               destination         
DROP       all  --  192.168.0.100        anywhere 
```

Alternative usage
-----------------
If you want block only HTTP attackers, you can replace:
```bash
netstat -ntu | awk '{print $5}' | cut -d: -f1 | sort | uniq -c | sort -nr > $BAD_IP_LIST
```
to
```bash
netstat -ntu | grep ":80" | awk '{print $5}' | cut -d: -f1 | sort | uniq -c | sort -nr > $BAD_IP_LIST
```
in `/usr/local/ddos/ddos.sh`

Updating
--------
```bash
cd /usr/local/ddos/
wget https://raw.githubusercontent.com/Amet13/ddos-deflate/master/ddos.sh -O ddos.sh
```

Uninstallation
--------------
```bash
su -
cd /tmp
wget https://raw.githubusercontent.com/Amet13/ddos-deflate/master/uninstall.sh
chmod +x uninstall.sh
./uninstall.sh
crontab -e
#*/1 * * * * /usr/local/ddos/ddos.sh >/dev/null 2>&1
```

Original author
---------------
[zaf@vsnl.com](mailto:zaf@vsnl.com)
