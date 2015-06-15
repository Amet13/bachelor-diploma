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
