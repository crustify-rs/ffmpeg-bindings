#!/bin/bash
# usage: wf_schedule.sh CFGROOT OUTROOT LU/SUB   -- one subsystem's wavefront API schedule
set -e
cfgroot=$1; outroot=$2; job=$3
cfg=$cfgroot/$job/wavefront-config.json
mkdir -p $outroot/$(dirname $job)
files=$(python3 -c "import json,sys;d=json.load(open('$cfg'));print(' '.join(d['api_headers']+d['impl_files']))")
PYTHONHASHSEED=0 wavefront /target --config $cfg schedule --output $outroot/$job.json \
  --file $files --transitive --api-headers-only \
  --max-types 2 --min-fields 20 --max-syms 25 --max-loc 500 \
  --skip av_assert0_fpu av_init_packet > $outroot/$job.log 2>&1 || { echo "FAIL $job"; tail -3 $outroot/$job.log; exit 1; }
tail -1 $outroot/$job.log
