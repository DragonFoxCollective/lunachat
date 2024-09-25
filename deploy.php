<?php

$repo = json_decode($_POST['payload'])->repository->name;

file_put_contents('deploy-log.txt', date('Y-m-d h:i:s') . ': ' . $repo . "\n", FILE_APPEND);

switch ($repo)
{
    case 'dragon-fox.com': $dir = '/var/www/dragon-fox.com'; break;
    default: die('Unhandled repo: ' . $repo);
}

exec('eval `ssh-agent`');
$output = [];
exec("cd $dir 2>&1 && git pull 2>&1", $output);
echo implode("\n", $output);
exec('kill $SSH_AGENT_PID');