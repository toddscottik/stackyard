<?php
declare(strict_types=1);
header('Content-Type: application/json');
echo json_encode([
    'fixture' => 'stackyard-php',
    'version' => PHP_VERSION,
    'openssl' => extension_loaded('openssl'),
    'mbstring' => extension_loaded('mbstring'),
], JSON_THROW_ON_ERROR);
