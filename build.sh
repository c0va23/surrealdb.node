napi build --platform --release $@
echo -e "import { ConnectionOptions } from \"./lib-src/embedded.js\";\n$(cat index.d.ts)" > index.d.ts
mkdir -p artifacts
mv surrealdb.node.*.node artifacts/
node move_artifacts.js
