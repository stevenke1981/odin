set -eu
packaging_revision="$1"
packaging_tree=/opt/odin-packaging-notices-02
packaging_manifest=/opt/odin-transfer/odin-notices-02-source-sha256.json
delivery=/opt/odin-delivery/v3-notices-02
binary_sha=e64c9a9e575b8d430f83acabd9a1b7503c9a1e7ed88ecd6215a0ca1bf33fba42
date -u +%Y-%m-%dT%H:%M:%SZ > "$delivery/notices-execution-start-utc.txt"
runuser -u odin-qa -- python3 /opt/odin-transfer/run-linux-doc-package-v3.py \
  --app-revision 0fe03ba30da9600c25588d506d7e094df89cf56f \
  --packaging-revision "$packaging_revision" --expected-binary-sha256 "$binary_sha" \
  --app-tree /opt/odin-linux-0fe03ba --packaging-tree "$packaging_tree" \
  --app-manifest /opt/odin-transfer/odin-0fe03ba-source-sha256.json --packaging-manifest "$packaging_manifest" \
  --delivery "$delivery" --output "$packaging_tree/dist/linux-notices-02"
python3 /opt/odin-transfer/run-linux-installer-v3.py \
  --revision "$packaging_revision" --app-revision 0fe03ba30da9600c25588d506d7e094df89cf56f \
  --tree "$packaging_tree" --manifest "$packaging_manifest" --delivery "$delivery" \
  --package-output "$packaging_tree/dist/linux-notices-02" --expected-binary-sha256 "$binary_sha"
date -u +%Y-%m-%dT%H:%M:%SZ > "$delivery/notices-execution-finished-before-export-utc.txt"
install -d -o odin-qa -g odin-qa /opt/odin-notice-export
runuser -u odin-qa -- python3 /opt/odin-transfer/export-linux-v3.py \
  --revision 0fe03ba30da9600c25588d506d7e094df89cf56f --tree /opt/odin-linux-0fe03ba \
  --manifest /opt/odin-transfer/odin-0fe03ba-source-sha256.json --delivery "$delivery" --expected-binary-sha256 "$binary_sha" \
  --packaging-revision "$packaging_revision" --packaging-tree "$packaging_tree" --packaging-manifest "$packaging_manifest" \
  --package-output "$packaging_tree/dist/linux-notices-02" --export /opt/odin-notice-export/v3-0fe03ba
