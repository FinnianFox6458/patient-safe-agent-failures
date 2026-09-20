#!/bin/sh
set -eu

curl --request POST http://127.0.0.1:3000/appointment-failures \
  --header 'Content-Type: application/json' \
  --data '{"appointment_ref":"apt_2048","stage":"confirmation","kind":"medication_risk","attempt":1,"patient_name":"Example Patient","patient_contact":"patient@example.test"}'

