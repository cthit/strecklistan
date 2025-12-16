#!/usr/bin/env sh

set -x

diesel setup

db_mock/populate.sh \
	--host db \
	--user postgres \
	--password password \
	--database strecklistan \
	--file db_mock/init.sql

cargo watch -x run
