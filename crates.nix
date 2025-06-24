{...}: {
  perSystem = {pkgs, ...}: {
    # declare projects
    # TODO: change this to your workspace's path
    nci.projects."my-project" = {
      path = ./.;
      # export all crates (packages and devshell) in flake outputs
      # alternatively you can access the outputs and export them yourself
      export = true;
    };
    # configure crates
    nci.crates = {
      "tinymist" = {
        drvConfig = {
          # env.HELLO_WORLD = true;
        };
        # look at documentation for more options
      };
      # "my-other-workspace-crate" = {
      #   drvConfig = {
      #     mkDerivation.buildInputs = [pkgs.hello];
      #   };
      #   # look at documentation for more options
      # };
    };
  };
}