FROM mysql:8.0

ENV MYSQL_ROOT_PASSWORD=r00t_TechPulse_S3cret
ENV MYSQL_DATABASE=techpulse_prod

COPY my.cnf /etc/mysql/conf.d/custom.cnf
COPY init.sql /docker-entrypoint-initdb.d/01-init.sql
COPY flag.txt /var/lib/mysql-files/flag.txt
RUN chmod 444 /var/lib/mysql-files/flag.txt

EXPOSE 3306
