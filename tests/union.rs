use async_graphql::*;

#[async_std::test]
pub async fn test_union_simple_object() {
    #[derive(SimpleObject)]
    struct MyObj {
        id: i32,
        title: String,
    }

    #[derive(Union)]
    enum Node {
        MyObj(MyObj),
    }

    struct Query;

    #[Object]
    impl Query {
        async fn node(&self) -> Node {
            MyObj {
                id: 33,
                title: "haha".to_string(),
            }
            .into()
        }
    }

    let query = r#"{
            node {
                ... on MyObj {
                    id
                }
            }
        }"#;
    let schema = Schema::new(Query, EmptyMutation, EmptySubscription);
    assert_eq!(
        schema.execute(query).await.into_result().unwrap().data,
        serde_json::json!({
            "node": {
                "id": 33,
            }
        })
    );
}

#[async_std::test]
pub async fn test_union_simple_object2() {
    #[derive(SimpleObject)]
    struct MyObj {
        id: i32,
        title: String,
    }

    #[derive(Union)]
    enum Node {
        MyObj(MyObj),
    }

    struct Query;

    #[Object]
    impl Query {
        async fn node(&self) -> Node {
            MyObj {
                id: 33,
                title: "haha".to_string(),
            }
            .into()
        }
    }

    let query = r#"{
            node {
                ... on MyObj {
                    id
                }
            }
        }"#;
    let schema = Schema::new(Query, EmptyMutation, EmptySubscription);
    assert_eq!(
        schema.execute(query).await.into_result().unwrap().data,
        serde_json::json!({
            "node": {
                "id": 33,
            }
        })
    );
}

#[async_std::test]
pub async fn test_multiple_unions() {
    struct MyObj;

    #[Object]
    impl MyObj {
        async fn value_a(&self) -> i32 {
            1
        }

        async fn value_b(&self) -> i32 {
            2
        }

        async fn value_c(&self) -> i32 {
            3
        }
    }

    #[derive(Union)]
    enum UnionA {
        MyObj(MyObj),
    }

    #[derive(Union)]
    enum UnionB {
        MyObj(MyObj),
    }

    struct Query;

    #[Object]
    impl Query {
        async fn union_a(&self) -> UnionA {
            MyObj.into()
        }
        async fn union_b(&self) -> UnionB {
            MyObj.into()
        }
    }

    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .register_type::<UnionA>() // `UnionA` is not directly referenced, so manual registration is required.
        .finish();
    let query = r#"{
            unionA {
               ... on MyObj {
                valueA
                valueB
                valueC
              }
            }
            unionB {
                ... on MyObj {
                 valueA
                 valueB
                 valueC
               }
             }
        }"#;
    assert_eq!(
        schema.execute(query).await.into_result().unwrap().data,
        serde_json::json!({
            "unionA": {
                "valueA": 1,
                "valueB": 2,
                "valueC": 3,
            },
            "unionB": {
                "valueA": 1,
                "valueB": 2,
                "valueC": 3,
            }
        })
    );
}

#[async_std::test]
pub async fn test_multiple_objects_in_multiple_unions() {
    struct MyObjOne;

    #[Object]
    impl MyObjOne {
        async fn value_a(&self) -> i32 {
            1
        }

        async fn value_b(&self) -> i32 {
            2
        }

        async fn value_c(&self) -> i32 {
            3
        }
    }

    struct MyObjTwo;

    #[Object]
    impl MyObjTwo {
        async fn value_a(&self) -> i32 {
            1
        }
    }

    #[derive(Union)]
    enum UnionA {
        MyObjOne(MyObjOne),
        MyObjTwo(MyObjTwo),
    }

    #[derive(Union)]
    enum UnionB {
        MyObjOne(MyObjOne),
    }

    struct Query;

    #[Object]
    impl Query {
        async fn my_obj(&self) -> Vec<UnionA> {
            vec![MyObjOne.into(), MyObjTwo.into()]
        }
    }

    let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
        .register_type::<UnionB>() // `UnionB` is not directly referenced, so manual registration is required.
        .finish();
    let query = r#"{
            myObj {
                ... on MyObjTwo {
                    valueA
                }
                ... on MyObjOne {
                    valueA
                    valueB
                    valueC
                }
            }
         }"#;
    assert_eq!(
        schema.execute(query).await.into_result().unwrap().data,
        serde_json::json!({
            "myObj": [{
                "valueA": 1,
                "valueB": 2,
                "valueC": 3,
            }, {
                "valueA": 1
            }]
        })
    );
}

#[async_std::test]
pub async fn test_union_field_result() {
    struct MyObj;

    #[Object]
    impl MyObj {
        async fn value(&self) -> FieldResult<i32> {
            Ok(10)
        }
    }

    #[derive(Union)]
    enum Node {
        MyObj(MyObj),
    }

    struct Query;

    #[Object]
    impl Query {
        async fn node(&self) -> Node {
            MyObj.into()
        }
    }

    let query = r#"{
            node {
                ... on MyObj {
                    value
                }
            }
        }"#;
    let schema = Schema::new(Query, EmptyMutation, EmptySubscription);
    assert_eq!(
        schema.execute(query).await.into_result().unwrap().data,
        serde_json::json!({
            "node": {
                "value": 10,
            }
        })
    );
}

#[async_std::test]
pub async fn test_union_flatten() {
    #[derive(SimpleObject)]
    struct MyObj1 {
        value1: i32,
    }

    #[derive(SimpleObject)]
    struct MyObj2 {
        value2: i32,
    }

    #[derive(Union)]
    enum InnerUnion1 {
        A(MyObj1),
    }

    #[derive(Union)]
    enum InnerUnion2 {
        B(MyObj2),
    }

    #[derive(Union)]
    enum MyUnion {
        #[graphql(flatten)]
        Inner1(InnerUnion1),

        #[graphql(flatten)]
        Inner2(InnerUnion2),
    }

    struct Query;

    #[Object]
    impl Query {
        async fn value1(&self) -> MyUnion {
            InnerUnion1::A(MyObj1 { value1: 99 }).into()
        }

        async fn value2(&self) -> MyUnion {
            InnerUnion2::B(MyObj2 { value2: 88 }).into()
        }
    }

    let schema = Schema::new(Query, EmptyMutation, EmptySubscription);
    let query = r#"
    {
        value1 {
            ... on MyObj1 {
                value1
            }
        }
        value2 {
            ... on MyObj2 {
                value2
            }
        }
    }"#;
    assert_eq!(
        schema.execute(query).await.into_result().unwrap().data,
        serde_json::json!({
            "value1": {
                "value1": 99,
            },
            "value2": {
                "value2": 88,
            }
        })
    );
}
