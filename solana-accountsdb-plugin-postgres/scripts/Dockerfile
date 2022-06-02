FROM postgres:14.0 AS builder

COPY create_schema.sql \
    drop_schema.sql \
    /opt/scripts/

COPY postgresql.conf /etc/postgresql/
COPY deploy.sh /docker-entrypoint-initdb.d/