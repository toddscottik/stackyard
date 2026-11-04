# PHP HTTP fixture

Copy this folder into a disposable working directory before installing dependencies. Select PHP 8.5 and Composer, import the copy, then apply a project profile enabling `openssl`. `deps install php-http` runs Composer and its platform check. Start the project and open its reported loopback port; JSON should contain `fixture: stackyard-php` and `openssl: true`.

An empty profile makes the Composer platform check fail without changing another project's profile. Enabling `mbstring` changes only this project's response. The optional `[site]` uses the existing Apache HTTP proxy; no FastCGI or PHP `.htaccess` behavior is implied. This is a hand-authored integration fixture, not the Stage 11 create-project recipe API.
