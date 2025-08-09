## TODO

Right now you need to either set udev externally or use sudo for openocd.
I want to set this automatically or do some magic with the user so that openocd can be used direction when using nix develop.

The lines in configuration.nix are:

```
  users.extraGroups.plugdev = { };
  users.extraUsers.conectado.extraGroups = [];
  services.udev.packages = [ pkgs.openocd ];
```
