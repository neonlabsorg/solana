FROM postgres:14-alpine

ENV PG_PARTMAN_VERSION v4.7.1
ENV PG_CRON_VERSION v1.4.2

RUN apk add --no-cache --virtual .fetch-deps \
        ca-certificates \
        openssl \
        tar

RUN apk add --no-cache --virtual .build-deps \
        autoconf \
        automake \
        g++ \
        clang \
        llvm \
        libtool \
        libxml2-dev \
        make \
        perl

# Install pg_partman
RUN set -ex \
    # Download pg_partman
    && wget -O pg_partman.tar.gz "https://github.com/pgpartman/pg_partman/archive/$PG_PARTMAN_VERSION.tar.gz" \
    # Create a folder to put the src files in
    && mkdir -p /usr/src/pg_partman \
    # Extract the src files
    && tar \
        --extract \
        --file pg_partman.tar.gz \
        --directory /usr/src/pg_partman \
        --strip-components 1 \
    # Delete src file tar
    && rm pg_partman.tar.gz \
    # Move to src file folder
    && cd /usr/src/pg_partman \
    # Build the extension
    && make \
    # Install the extension
    && make install \
    # Delete the src files for pg_partman
    && rm -rf /usr/src/pg_partman

#Installing pg_cron
RUN set -ex \
    # Download pg_cron
    && wget -O pg_cron.tar.gz "https://github.com/citusdata/pg_cron/archive/$PG_CRON_VERSION.tar.gz" \
    # Create a folder to put the src files in
    && mkdir -p /usr/src/pg_cron \
    # Extract src files
    && tar \
        --extract \
        --file pg_cron.tar.gz \
        --directory /usr/src/pg_cron \
        --strip-components 1 \
    # Delete src file tar
    && rm pg_cron.tar.gz \
    # Move to src file folder
    && cd /usr/src/pg_cron \
    # Build the extension
    && make \
    # Install the extension
    && make install \
    # Delete the src files for pg_cron
    && rm -rf /usr/src/pg_cron \
    # Delete the dependancies for downloading and building the extensions, we no longer need them
    && apk del .fetch-deps .build-deps

COPY create_schema.sql \
    create_functions.sql \
    drop_schema.sql \
    /opt/scripts/

COPY postgresql.conf /etc/postgresql/
COPY deploy.sh /docker-entrypoint-initdb.d/
