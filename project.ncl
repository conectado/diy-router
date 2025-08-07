let inputs = import "./nickel.lock.ncl" in
let organist = inputs.organist in

organist.OrganistExpression
& {
  Schema,
  config | Schema
    = {
      shells = organist.shells.Rust,

      shells.build = {
        packages = {},
      },

      shells.dev = {
        packages = {}
      },
    },
}
  | organist.modules.T
