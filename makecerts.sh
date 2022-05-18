set -e
mkdir -p certs
cp echo.test.ext certs/
openssl genrsa -out certs/rootCA.key 4096
openssl req -x509 -new -nodes -key certs/rootCA.key -sha256 -days 1024 -out certs/rootCA.crt
openssl genrsa -out certs/echo.test.key 4096
openssl req -new -key certs/echo.test.key -out certs/echo.test.csr
openssl x509 -req -in certs/echo.test.csr -CA certs/rootCA.crt -CAkey certs/rootCA.key -CAcreateserial -out certs/echo.test.crt -days 825 -sha256 -extfile certs/echo.test.ext