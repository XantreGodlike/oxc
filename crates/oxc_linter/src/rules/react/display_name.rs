use std::fmt::format;

use oxc_ast::{
    ast::{Argument, BindingPatternKind, Expression, VariableDeclaration},
    syntax_directed_operations::PropName,
    AstKind,
};
use oxc_diagnostics::{
    miette::{self, Diagnostic},
    thiserror::{self, Error},
};
use oxc_macros::declare_oxc_lint;
use oxc_span::{GetSpan, Span};
use serde::{de, Deserialize, Deserializer};

use crate::{context::LintContext, rule::Rule, AstNode};

#[derive(Debug, Error, Diagnostic)]
#[error("eslint-plugin-react(display-name):")]
#[diagnostic(severity(warning), help(""))]
struct DisplayNameDiagnostic(#[label] pub Span);

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayName {
    #[serde(default)]
    check_context_objects: bool,
    #[serde(default)]
    ignore_transpiler_name: bool,
}

declare_oxc_lint!(
    /// ### What it does
    ///
    ///
    /// ### Why is this bad?
    ///
    ///
    /// ### Example
    /// ```javascript
    /// ```
    DisplayName,
    correctness
);

#[derive(Debug)]
enum ComponentType {
    Named,
    TranspilerNamed,
    Unnamed,
}

fn get_expr_ident(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(ident) => Some(ident.name.to_string()),
        Expression::MemberExpression(member_expr) => {
            let Some(parent_name) = get_expr_ident(member_expr.object()) else { return None };
            member_expr.static_property_name().map(|it| format!("{}.{}", parent_name, it))
        }
        _ => None,
    }
}

fn get_component_type(node: &AstKind) -> Option<ComponentType> {
    match &node {
        AstKind::VariableDeclarator(var_decl) => {
            let var_name = match &var_decl.id.kind {
                BindingPatternKind::BindingIdentifier(ident) => Some(ident.name.to_string()),
                _ => None,
            };
            #[cfg(debug_assertions)]
            println!("var_decl {:?}", var_name);
            let Some(init) = &var_decl.init else { return None };

            match init {
                Expression::CallExpression(call_expr) => {
                    let Some(ident) = get_expr_ident(&call_expr.callee) else {
                        return None;
                    };

                    if ident != "createReactClass"
                        && ident != "createClass"
                        && ident != "React.createClass"
                    {
                        return None;
                    }
                    #[cfg(debug_assertions)]
                    println!("createClassName detected");

                    match &call_expr.arguments.as_slice() {
                        [Argument::Expression(Expression::ObjectExpression(obj_expr))] => {
                            let has_display_name = obj_expr.properties.iter().any(|it| {
                                #[cfg(debug_assertions)]
                                println!(
                                    "prop-name {:?}",
                                    it.prop_name().map_or("_", |name| name.0)
                                );
                                it.prop_name().map_or(false, |name| name.0 == "displayName")
                            });
                            if has_display_name {
                                Some(ComponentType::Named)
                            } else {
                                Some(ComponentType::TranspilerNamed)
                            }
                        }
                        _ => None,
                    }
                },
                _ => None,
            }
        }
        _ => None,
    }
}

trait DeserializeConfig {
    fn config<T: for<'a> Deserialize<'a>>(self) -> Option<T>;
}
impl DeserializeConfig for serde_json::Value {
    fn config<T: for<'a> Deserialize<'a>>(self) -> Option<T> {
        self.as_array().and_then(|it| match it.as_slice() {
            [value] => T::deserialize(value).map_or(None, |it| Some(it)),
            _ => None,
        })
    }
}

impl Rule for DisplayName {
    fn from_configuration(_value: serde_json::Value) -> Self {
        _value
            .config()
            .unwrap_or(Self { check_context_objects: false, ignore_transpiler_name: false })
    }

    fn run_on_symbol(&self, _symbol_id: oxc_semantic::SymbolId, _ctx: &LintContext<'_>) {
        let declaration_id = _ctx.symbols().get_declaration(_symbol_id);

        let node = _ctx.nodes().get_node(declaration_id);

        let Some(component_type) = get_component_type(&node.kind()) else { return };

        println!(
            "ignore transpiler name: {:?}; component_type {:?}",
            self.ignore_transpiler_name, component_type
        );
        if match component_type {
            ComponentType::TranspilerNamed => self.ignore_transpiler_name,
            ComponentType::Unnamed => true,
            _ => false,
        } {
            _ctx.diagnostic(DisplayNameDiagnostic(node.kind().span()));
        }
    }
    /* fn run_once(&self, _ctx: &LintContext) {
        for node in _ctx.nodes().iter() {

        }
    } */
}

#[test]
fn test() {
    use crate::tester::Tester;

    let pass = vec![
        (
            r#"
			        var Hello = createReactClass({
			          displayName: 'Hello',
			          render: function() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        var Hello = React.createClass({
			          displayName: 'Hello',
			          render: function() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            Some(serde_json::json!({
              "react": {
                "createClass": "createClass",
              },
            })),
        ),
        (
            r#"
			        class Hello extends React.Component {
			          render() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        }
			        Hello.displayName = 'Hello'
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        class Hello {
			          render() {
			            return 'Hello World';
			          }
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        class Hello extends Greetings {
			          static text = 'Hello World';
			          render() {
			            return Hello.text;
			          }
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        class Hello {
			          method;
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        class Hello extends React.Component {
			          static get displayName() {
			            return 'Hello';
			          }
			          render() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        }
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        class Hello extends React.Component {
			          static displayName = 'Widget';
			          render() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        }
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        var Hello = createReactClass({
			          render: function() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        });
			      "#,
            None,
            None,
        ),
        (
            r#"
			        class Hello extends React.Component {
			          render() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        export default class Hello {
			          render() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        var Hello;
			        Hello = createReactClass({
			          render: function() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        });
			      "#,
            None,
            None,
        ),
        (
            r#"
			        module.exports = createReactClass({
			          "displayName": "Hello",
			          "render": function() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        });
			      "#,
            None,
            None,
        ),
        (
            r#"
			        var Hello = createReactClass({
			          displayName: 'Hello',
			          render: function() {
			            let { a, ...b } = obj;
			            let c = { ...d };
			            return <div />;
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        export default class {
			          render() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        export const Hello = React.memo(function Hello() {
			          return <p />;
			        })
			      "#,
            None,
            None,
        ),
        (
            r#"
			        var Hello = function() {
			          return <div>Hello {this.props.name}</div>;
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        function Hello() {
			          return <div>Hello {this.props.name}</div>;
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        var Hello = () => {
			          return <div>Hello {this.props.name}</div>;
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        module.exports = function Hello() {
			          return <div>Hello {this.props.name}</div>;
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        function Hello() {
			          return <div>Hello {this.props.name}</div>;
			        }
			        Hello.displayName = 'Hello';
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        var Hello = () => {
			          return <div>Hello {this.props.name}</div>;
			        }
			        Hello.displayName = 'Hello';
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        var Hello = function() {
			          return <div>Hello {this.props.name}</div>;
			        }
			        Hello.displayName = 'Hello';
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        var Mixins = {
			          Greetings: {
			            Hello: function() {
			              return <div>Hello {this.props.name}</div>;
			            }
			          }
			        }
			        Mixins.Greetings.Hello.displayName = 'Hello';
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        var Hello = createReactClass({
			          render: function() {
			            return <div>{this._renderHello()}</div>;
			          },
			          _renderHello: function() {
			            return <span>Hello {this.props.name}</span>;
			          }
			        });
			      "#,
            None,
            None,
        ),
        (
            r#"
			        var Hello = createReactClass({
			          displayName: 'Hello',
			          render: function() {
			            return <div>{this._renderHello()}</div>;
			          },
			          _renderHello: function() {
			            return <span>Hello {this.props.name}</span>;
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        const Mixin = {
			          Button() {
			            return (
			              <button />
			            );
			          }
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        var obj = {
			          pouf: function() {
			            return any
			          }
			        };
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        var obj = {
			          pouf: function() {
			            return any
			          }
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        export default {
			          renderHello() {
			            let {name} = this.props;
			            return <div>{name}</div>;
			          }
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React, { createClass } from 'react';
			        export default createClass({
			          displayName: 'Foo',
			          render() {
			            return <h1>foo</h1>;
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            Some(serde_json::json!({
              "react": {
                "createClass": "createClass",
              },
            })),
        ),
        (
            r#"
			        import React, {Component} from "react";
			        function someDecorator(ComposedComponent) {
			          return class MyDecorator extends Component {
			            render() {return <ComposedComponent {...this.props} />;}
			          };
			        }
			        module.exports = someDecorator;
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React, {createElement} from "react";
			        const SomeComponent = (props) => {
			          const {foo, bar} = props;
			          return someComponentFactory({
			            onClick: () => foo(bar("x"))
			          });
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React, {createElement} from "react";
			        const SomeComponent = (props) => {
			          const {foo, bar} = props;
			          return someComponentFactory({
			            onClick: () => foo(bar("x"))
			          });
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React, {Component} from "react";
			        function someDecorator(ComposedComponent) {
			          return class MyDecorator extends Component {
			            render() {return <ComposedComponent {...this.props} />;}
			          };
			        }
			        module.exports = someDecorator;
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React, {Component} from "react";
			        function someDecorator(ComposedComponent) {
			          return class MyDecorator extends Component {
			            render() {return <ComposedComponent {...this.props} />;}
			          };
			        }
			        module.exports = someDecorator;
			      "#,
            None,
            None,
        ),
        (
            r#"
			        const element = (
			          <Media query={query} render={() => {
			            renderWasCalled = true
			            return <div/>
			          }}/>
			        )
			      "#,
            None,
            None,
        ),
        (
            r#"
			        const element = (
			          <Media query={query} render={function() {
			            renderWasCalled = true
			            return <div/>
			          }}/>
			        )
			      "#,
            None,
            None,
        ),
        (
            r#"
			        module.exports = {
			          createElement: tagName => document.createElement(tagName)
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        const { createElement } = document;
			        createElement("a");
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from 'react'
			        import { string } from 'prop-types'
			
			        function Component({ world }) {
			          return <div>Hello {world}</div>
			        }
			
			        Component.propTypes = {
			          world: string,
			        }
			
			        export default React.memo(Component)
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from 'react'
			
			        const ComponentWithMemo = React.memo(function Component({ world }) {
			          return <div>Hello {world}</div>
			        })
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from 'react';
			
			        const Hello = React.memo(function Hello() {
			          return;
			        });
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from 'react'
			
			        const ForwardRefComponentLike = React.forwardRef(function ComponentLike({ world }, ref) {
			          return <div ref={ref}>Hello {world}</div>
			        })
			      "#,
            None,
            None,
        ),
        (
            r#"
			        function F() {
			          let items = [];
			          let testData = [
			            {a: "test1", displayName: "test2"}, {a: "test1", displayName: "test2"}];
			          for (let item of testData) {
			              items.push({a: item.a, b: item.displayName});
			          }
			          return <div>{items}</div>;
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import {Component} from "react";
			        type LinkProps = {};
			        class Link extends Component<LinkProps> {}
			      "#,
            None,
            None,
        ),
        (
            r#"
			        const x = {
			          title: "URL",
			          dataIndex: "url",
			          key: "url",
			          render: url => (
			            <a href={url} target="_blank" rel="noopener noreferrer">
			              <p>lol</p>
			            </a>
			          )
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        const renderer = a => function Component(listItem) {
			          return <div>{a} {listItem}</div>;
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        const Comp = React.forwardRef((props, ref) => <main />);
			        Comp.displayName = 'MyCompName';
			      "#,
            None,
            None,
        ),
        (
            r#"
			        const Comp = React.forwardRef((props, ref) => <main data-as="yes" />) as SomeComponent;
			        Comp.displayName = 'MyCompNameAs';
			      "#,
            None,
            None,
        ),
        (
            r#"
			        function Test() {
			          const data = [
			            {
			              name: 'Bob',
			            },
			          ];
			
			          const columns = [
			            {
			              Header: 'Name',
			              accessor: 'name',
			              Cell: ({ value }) => <div>{value}</div>,
			            },
			          ];
			
			          return <ReactTable columns={columns} data={data} />;
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        const f = (a) => () => {
			          if (a) {
			            return null;
			          }
			          return 1;
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        class Test {
			          render() {
			            const data = [
			              {
			                name: 'Bob',
			              },
			            ];
			
			            const columns = [
			              {
			                Header: 'Name',
			                accessor: 'name',
			                Cell: ({ value }) => <div>{value}</div>,
			              },
			            ];
			
			            return <ReactTable columns={columns} data={data} />;
			          }
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        export const demo = (a) => (b) => {
			          if (a == null) return null;
			          return b;
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        let demo = null;
			        demo = (a) => {
			          if (a == null) return null;
			          return f(a);
			        };"#,
            None,
            None,
        ),
        (
            r#"
			        obj._property = (a) => {
			          if (a == null) return null;
			          return f(a);
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        _variable = (a) => {
			          if (a == null) return null;
			          return f(a);
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        demo = () => () => null;
			      "#,
            None,
            None,
        ),
        (
            r#"
			        demo = {
			          property: () => () => null
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        demo = function() {return function() {return null;};};
			      "#,
            None,
            None,
        ),
        (
            r#"
			        demo = {
			          property: function() {return function() {return null;};}
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        function MyComponent(props) {
			          return <b>{props.name}</b>;
			        }
			
			        const MemoizedMyComponent = React.memo(
			          MyComponent,
			          (prevProps, nextProps) => prevProps.name === nextProps.name
			        )
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from 'react'
			
			        const MemoizedForwardRefComponentLike = React.memo(
			          React.forwardRef(function({ world }, ref) {
			            return <div ref={ref}>Hello {world}</div>
			        })
			        )
			      "#,
            None,
            Some(serde_json::json!({
              "react": {
                "version": "16.14.0",
              },
            })),
        ),
        (
            r#"
			        import React from 'react'
			
			        const MemoizedForwardRefComponentLike = React.memo(
			          React.forwardRef(({ world }, ref) => {
			            return <div ref={ref}>Hello {world}</div>
			          })
			        )
			      "#,
            None,
            Some(serde_json::json!({
              "react": {
                "version": "15.7.0",
              },
            })),
        ),
        (
            r#"
			        import React from 'react'
			
			        const MemoizedForwardRefComponentLike = React.memo(
			          React.forwardRef(function ComponentLike({ world }, ref) {
			            return <div ref={ref}>Hello {world}</div>
			          })
			        )
			      "#,
            None,
            Some(serde_json::json!({
              "react": {
                "version": "16.12.1",
              },
            })),
        ),
        (
            r#"
			        export const ComponentWithForwardRef = React.memo(
			          React.forwardRef(function Component({ world }) {
			            return <div>Hello {world}</div>
			          })
			        )
			      "#,
            None,
            Some(serde_json::json!({
              "react": {
                "version": "0.14.11",
              },
            })),
        ),
        (
            r#"
			        import React from 'react'
			
			        const MemoizedForwardRefComponentLike = React.memo(
			          React.forwardRef(function({ world }, ref) {
			            return <div ref={ref}>Hello {world}</div>
			          })
			        )
			      "#,
            None,
            Some(serde_json::json!({
              "react": {
                "version": "15.7.1",
              },
            })),
        ),
        (
            r#"
			        import React from 'react';
			
			        const Hello = React.createContext();
			        Hello.displayName = "HelloContext"
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        const Hello = createContext();
			        Hello.displayName = "HelloContext"
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        const Hello = createContext();
			
			        const obj = {};
			        obj.displayName = "False positive";
			
			        Hello.displayName = "HelloContext"
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        import * as React from 'react';
			
			        const Hello = React.createContext();
			
			        const obj = {};
			        obj.displayName = "False positive";
			
			        Hello.displayName = "HelloContext";
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        const obj = {};
			        obj.displayName = "False positive";
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        const Hello = createContext();
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            Some(serde_json::json!({
              "react": {
                "version": "16.2.0",
              },
            })),
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        const Hello = createContext();
			        Hello.displayName = "HelloContext";
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            Some(serde_json::json!({
              "react": {
                "version": ">16.3.0",
              },
            })),
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        let Hello;
			        Hello = createContext();
			        Hello.displayName = "HelloContext";
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        const Hello = createContext();
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": false }])),
            Some(serde_json::json!({
              "react": {
                "version": ">16.3.0",
              },
            })),
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        var Hello;
			        Hello = createContext();
			        Hello.displayName = "HelloContext";
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        var Hello;
			        Hello = React.createContext();
			        Hello.displayName = "HelloContext";
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
    ];

    let fail = vec![
        (
            r#"
			        var Hello = createReactClass({
			          render: function() {
			            return React.createElement("div", {}, "text content");
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        var Hello = React.createClass({
			          render: function() {
			            return React.createElement("div", {}, "text content");
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            Some(serde_json::json!({
              "react": {
                "createClass": "createClass",
              },
            })),
        ),
        (
            r#"
			        var Hello = createReactClass({
			          render: function() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        class Hello extends React.Component {
			          render() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        }
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        function HelloComponent() {
			          return createReactClass({
			            render: function() {
			              return <div>Hello {this.props.name}</div>;
			            }
			          });
			        }
			        module.exports = HelloComponent();
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        module.exports = () => {
			          return <div>Hello {props.name}</div>;
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        module.exports = function() {
			          return <div>Hello {props.name}</div>;
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        module.exports = createReactClass({
			          render() {
			            return <div>Hello {this.props.name}</div>;
			          }
			        });
			      "#,
            None,
            None,
        ),
        (
            r#"
			        var Hello = createReactClass({
			          _renderHello: function() {
			            return <span>Hello {this.props.name}</span>;
			          },
			          render: function() {
			            return <div>{this._renderHello()}</div>;
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        var Hello = Foo.createClass({
			          _renderHello: function() {
			            return <span>Hello {this.props.name}</span>;
			          },
			          render: function() {
			            return <div>{this._renderHello()}</div>;
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            Some(serde_json::json!({
              "react": {
                "pragma": "Foo",
                "createClass": "createClass",
              },
            })),
        ),
        (
            r#"
			        /** @jsx Foo */
			        var Hello = Foo.createClass({
			          _renderHello: function() {
			            return <span>Hello {this.props.name}</span>;
			          },
			          render: function() {
			            return <div>{this._renderHello()}</div>;
			          }
			        });
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            Some(serde_json::json!({
              "react": {
                "createClass": "createClass",
              },
            })),
        ),
        (
            r#"
			        const Mixin = {
			          Button() {
			            return (
			              <button />
			            );
			          }
			        };
			      "#,
            Some(serde_json::json!([{ "ignoreTranspilerName": true }])),
            None,
        ),
        (
            r#"
			        function Hof() {
			          return function () {
			            return <div />
			          }
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React, { createElement } from "react";
			        export default (props) => {
			          return createElement("div", {}, "hello");
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from 'react'
			
			        const ComponentWithMemo = React.memo(({ world }) => {
			          return <div>Hello {world}</div>
			        })
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from 'react'
			
			        const ComponentWithMemo = React.memo(function() {
			          return <div>Hello {world}</div>
			        })
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from 'react'
			
			        const ForwardRefComponentLike = React.forwardRef(({ world }, ref) => {
			          return <div ref={ref}>Hello {world}</div>
			        })
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from 'react'
			
			        const ForwardRefComponentLike = React.forwardRef(function({ world }, ref) {
			          return <div ref={ref}>Hello {world}</div>
			        })
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from 'react'
			
			        const MemoizedForwardRefComponentLike = React.memo(
			          React.forwardRef(({ world }, ref) => {
			            return <div ref={ref}>Hello {world}</div>
			          })
			        )
			      "#,
            None,
            Some(serde_json::json!({
              "react": {
                "version": "15.6.0",
              },
            })),
        ),
        (
            r#"
			        import React from 'react'
			
			        const MemoizedForwardRefComponentLike = React.memo(
			          React.forwardRef(function({ world }, ref) {
			            return <div ref={ref}>Hello {world}</div>
			          })
			        )
			      "#,
            None,
            Some(serde_json::json!({
              "react": {
                "version": "0.14.2",
              },
            })),
        ),
        (
            r#"
			        import React from 'react'
			
			        const MemoizedForwardRefComponentLike = React.memo(
			          React.forwardRef(function ComponentLike({ world }, ref) {
			            return <div ref={ref}>Hello {world}</div>
			          })
			        )
			      "#,
            None,
            Some(serde_json::json!({
              "react": {
                "version": "15.0.1",
              },
            })),
        ),
        (
            r#"
			        import React from "react";
			        const { createElement } = React;
			        export default (props) => {
			          return createElement("div", {}, "hello");
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        import React from "react";
			        const createElement = React.createElement;
			        export default (props) => {
			          return createElement("div", {}, "hello");
			        };
			      "#,
            None,
            None,
        ),
        (
            r#"
			        module.exports = function () {
			          function a () {}
			          const b = function b () {}
			          const c = function () {}
			          const d = () => {}
			          const obj = {
			            a: function a () {},
			            b: function b () {},
			            c () {},
			            d: () => {},
			          }
			          return React.createElement("div", {}, "text content");
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        module.exports = () => {
			          function a () {}
			          const b = function b () {}
			          const c = function () {}
			          const d = () => {}
			          const obj = {
			            a: function a () {},
			            b: function b () {},
			            c () {},
			            d: () => {},
			          }
			
			          return React.createElement("div", {}, "text content");
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        export default class extends React.Component {
			          render() {
			            function a () {}
			            const b = function b () {}
			            const c = function () {}
			            const d = () => {}
			            const obj = {
			              a: function a () {},
			              b: function b () {},
			              c () {},
			              d: () => {},
			            }
			            return <div>Hello {this.props.name}</div>;
			          }
			        }
			      "#,
            None,
            None,
        ),
        (
            r#"
			        export default class extends React.PureComponent {
			          render() {
			            return <Card />;
			          }
			        }
			
			        const Card = (() => {
			          return React.memo(({ }) => (
			            <div />
			          ));
			        })();
			      "#,
            None,
            None,
        ),
        (
            r#"
			        const renderer = a => listItem => (
			          <div>{a} {listItem}</div>
			        );
			      "#,
            None,
            None,
        ),
        (
            r#"
			        const processData = (options?: { value: string }) => options?.value || 'no data';
			
			        export const Component = observer(() => {
			          const data = processData({ value: 'data' });
			          return <div>{data}</div>;
			        });
			
			        export const Component2 = observer(() => {
			          const data = processData();
			          return <div>{data}</div>;
			        });
			      "#,
            None,
            Some(serde_json::json!({ "componentWrapperFunctions": ["observer"] })),
        ),
        (
            r#"
			        import React from 'react';
			
			        const Hello = React.createContext();
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        import * as React from 'react';
			
			        const Hello = React.createContext();
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        const Hello = createContext();
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        var Hello;
			        Hello = createContext();
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
        (
            r#"
			        import { createContext } from 'react';
			
			        var Hello;
			        Hello = React.createContext();
			      "#,
            Some(serde_json::json!([{ "checkContextObjects": true }])),
            None,
        ),
    ];

    Tester::new(DisplayName::NAME, pass, fail).test_and_snapshot();
}
